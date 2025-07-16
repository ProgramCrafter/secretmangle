pub mod xor_intrinsic;

use std::sync::atomic::{fence, Ordering};
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr::NonNull;

/// XORs the data behind first pointer using key from second pointer.
/// The mangling operation is guaranteed to not be reordered after
/// any later operation, by usage of atomic fence with SeqCst semantics.
/// (See <https://github.com/RustCrypto/utils/blob/34c554f13500dd11566922048d6e865787d6fa51/zeroize/src/lib.rs#L301-L304>
/// for more details.)
///
/// # Safety
/// - `data` and `key` must be correctly aligned for `T`
/// - `data` and `key` must have at least `size_of::<T>()` bytes allocated
/// - `data` and `key` must either be non-overlapping or the same
///
/// No requirements on initialization status are made.
unsafe fn xor_chunks<T>(data: *mut u8, key: *const u8) {
    unsafe {
        xor_intrinsic::xor_chunks_intrinsic_baseline::<T>(data, key);
    }
    fence(Ordering::SeqCst);
}

/// Utility for masking a structure in program's heap with a random key,
/// supporting an arbitrary content type.
///
/// This version is written using assembly (not even common Unsafe Rust).
/// If your data is [`bytemuck::NoUninit`] (that is, Copy and has no padding), you can
/// also use [`crate::MangledBox`].
///
/// It is recommended to use [`std::clone::CloneToUninit`] to initialize
/// the contents of the box rather than constructing it on stack, since the
/// latter option might leave some trace of value being masked.
pub struct MangledBoxArbitrary<T> {
    /// Heap allocation with bytes mangled by XORing with `key`.
    data: Box<MaybeUninit<T>>,

    /// T-sized buffer containing a cryptographically secure random key.
    key: MaybeUninit<T>,
}

impl<T> MangledBoxArbitrary<T> {
    /// Constructs a new [`MangledBoxArbitrary`] with a random key and arbitrary data.
    pub fn new() -> Self {
        let data = Box::new_zeroed();
        // ^ [`data`] starts with arbitrary data from perspective of outer
        //   program; therefore we may choose anything, including that the block
        //   might had data equal to key (their XOR being zero).

        let mut key = MaybeUninit::uninit();
        getrandom::fill_uninit(key.as_bytes_mut()).expect("no keygen");
        // ^ fill_uninit guarantees that [`key`] is fully initialized on success

        Self { data, key }
    }

    /// Rekeys the box, preserving its contents.
    pub fn rekey(&mut self) {
        let mut diff_key = MaybeUninit::<T>::uninit();
        getrandom::fill_uninit(diff_key.as_bytes_mut()).expect("no keygen");

        unsafe {
            xor_chunks::<T>(
                Box::as_mut_ptr(&mut self.data).cast::<u8>(),
                diff_key.as_ptr().cast::<u8>(),
            );
            xor_chunks::<T>(
                self.key.as_mut_ptr().cast::<u8>(),
                diff_key.as_ptr().cast::<u8>(),
            );
        }
    }

    pub(crate) fn with_mangled<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(NonNull<T>) -> R {
        
        let data_ptr: *mut T = Box::as_mut_ptr(&mut self.data).cast::<T>();
        f(NonNull::new(data_ptr).unwrap())
    }

    /// Unmangles the contents and invokes the provided closure on it.
    /// Whether the closure panics or returns normally, the contents
    /// are remangled.
    pub fn with_unmangled<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(NonNull<T>) -> R,
    {
        let data_ptr = Box::as_mut_ptr(&mut self.data).cast::<u8>();
        let key_ptr = self.key.as_ptr().cast::<u8>();

        // Never panics as that's a pointer into Box allocation.
        // Compiler is probably able to optimize this check out.
        let data_nn: NonNull<u8> = NonNull::new(data_ptr).unwrap();

        // # Safety
        // 1. Both pointers point to some `MaybeUninit<T>`, so aligned
        // 2. [`data_ptr`] and [`key_ptr`] point to an allocation of at least
        //    `size_of::<T>()` bytes because they are obtained from references
        //    to `MaybeUninit<T>`.
        // 3. [`data_ptr`] points to heap allocation and [`key_ptr`] to
        //    stack, therefore they do not overlap.
        unsafe {
            xor_chunks::<T>(data_ptr, key_ptr);
        }

        /// Structure that handles remangling the pointed-to memory when
        /// dropped (both upon panic and successful [`with_unmangled`]
        /// completion). It is scoped because it is unsafe to construct.
        struct RemangleGuard<T> {
            data: *mut u8,
            key: *const u8,
            token: PhantomData<T>,
        }
        impl<T> Drop for RemangleGuard<T> {
            fn drop(&mut self) {
                unsafe { xor_chunks::<T>(self.data, self.key) }
            }
        }

        // # Safety
        // 1. Both pointers point to some `MaybeUninit<T>`, so aligned
        // 2. [`data_ptr`] and [`key_ptr`] point to an allocation of at least
        //    `size_of::<T>()` bytes because they are obtained from references
        //    to `MaybeUninit<T>`.
        // 3. [`data_ptr`] points to heap allocation and [`key_ptr`] to
        //    stack, therefore they do not overlap.
        let _guard = RemangleGuard::<T> {
            data: data_ptr,
            key: key_ptr,
            token: PhantomData,
        };

        f(data_nn.cast())
    }

    /// Drops the contents of the box, leaving it logically uninitialized.
    ///
    /// Using this is required to run any internal destructors, because the
    /// Drop implementation cannot know if there is any value to destroy.
    ///
    /// # Safety
    /// [`Self::with_unmangled`] must have initialized the contents.
    pub unsafe fn drop_in_place(&mut self) {
        self.with_unmangled(|p| unsafe { p.drop_in_place() });
    }
}

impl<T> Default for MangledBoxArbitrary<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for MangledBoxArbitrary<T> {
    fn drop(&mut self) {
        let data_ptr = Box::as_mut_ptr(&mut self.data).cast::<u8>();
        let key_ptr = self.key.as_mut_ptr().cast::<u8>();

        // # Safety
        // 1. Both pointers point to some `MaybeUninit<T>`, so aligned
        // 2. Both pointers were obtained from `&mut MaybeUninit<T>`
        //    to an allocation of at least `size_of::<T>()`.
        // 3. Each call passes the same pointer in both arguments.
        unsafe {
            xor_chunks::<T>(data_ptr, data_ptr);
            xor_chunks::<T>(key_ptr, key_ptr);
        }
    }
}

#[cfg(all(test, not(miri)))]
mod tests {
    use std::clone::CloneToUninit;
    use std::cell::RefCell;
    use std::ptr::NonNull;
    use std::rc::Rc;

    use super::MangledBoxArbitrary as MangledBox;

    fn ensure_send<T: Send>(_v: &T) {}
    fn ensure_sync<T: Sync>(_v: &T) {}

    #[test]
    fn zst() {
        let mut empty_box = MangledBox::<()>::new();
        ensure_send(&empty_box);
        ensure_sync(&empty_box);

        empty_box.with_unmangled(|_| {});
    }

    #[derive(Clone, Copy)]
    #[repr(C, align(64))]
    struct Align64;

    #[test]
    fn overaligned_zst() {
        let mut align64_box = MangledBox::<Align64>::new();
        ensure_send(&align64_box);
        ensure_sync(&align64_box);

        align64_box.with_unmangled(|p| {
            assert_eq!(
                p.as_ptr().align_offset(64),
                0,
                "alignment not preserved on overaligned ZST type"
            );
        });
    }

    struct ReportDrop(Rc<RefCell<bool>>);
    impl Drop for ReportDrop {
        fn drop(&mut self) {
            *self.0.borrow_mut() = true;
        }
    }

    #[test]
    fn drop_at_correct_time() {
        let drop_reported = Rc::new(RefCell::new(false));
        let drop_reported_clone = drop_reported.clone();

        {
            let mut box_ = MangledBox::<ReportDrop>::new();
            box_.with_unmangled(|p: NonNull<ReportDrop>| {
                // We DO NOT construct `ReportDrop` on stack.
                // `let val = ReportDrop(drop_reported_clone);`
                // would be a semantic misuse of this mangled box.

                let place: *mut u8 = p.as_ptr().cast();
                // Safety: `with_unmangled` promises that [`place`] points
                // to an allocation valid for `ReportDrop`.
                // `clone_to_uninit` does not require that [`place`] is
                // initialized beforehand, nor does `with_unmangled` require
                // that [`place`] is initialized after closure exits.
                unsafe { drop_reported_clone.clone_to_uninit(place) };
            });
            assert!(!*drop_reported.borrow(), "dropped a live box");

            unsafe {
                box_.drop_in_place();
            }
            assert!(*drop_reported.borrow(), "did not receive drop report");
        }
    }

    #[test]
    fn no_auto_drop() {
        let drop_reported = Rc::new(RefCell::new(false));
        let drop_reported_clone = drop_reported.clone();

        {
            let mut box_ = MangledBox::<ReportDrop>::new();
            box_.with_unmangled(|p: NonNull<ReportDrop>| {
                // We DO NOT construct `ReportDrop` on stack.
                // `let val = ReportDrop(drop_reported_clone);`
                // would be a semantic misuse of this mangled box.

                let place: *mut u8 = p.as_ptr().cast();
                // Safety: `with_unmangled` promises that [`place`] points
                // to an allocation valid for `ReportDrop`.
                // `clone_to_uninit` does not require that [`place`] is
                // initialized beforehand, nor does `with_unmangled` require
                // that [`place`] is initialized after closure exits.
                unsafe { drop_reported_clone.clone_to_uninit(place) };
            });
            assert!(!*drop_reported.borrow(), "dropped a live box");

            // Now, do not drop the contents but drop the `box_` itself.
        }
        assert!(
            !*drop_reported.borrow(),
            "box forwarded drop when it could not prove content is initialized"
        );
    }

    #[test]
    fn real_structures_string() {
        use std::fmt::Write;

        let mut box_ = MangledBox::<String>::new();
        box_.with_unmangled(|p| unsafe {
            p.write("hello".to_owned());
        });
        box_.with_unmangled(|mut p| {
            assert_eq!(unsafe { p.as_ref() }, "hello");
            unsafe {
                p.as_mut().push_str(" Rust!");
            }
        });
        box_.rekey();
        box_.with_unmangled(|p| {
            let mut s = String::with_capacity(13);
            write!(s, "{:?}", unsafe { p.as_ref() }).unwrap();
            assert_eq!(s, "\"hello Rust!\"");
        });
        unsafe {
            box_.drop_in_place();
        }
    }
}
