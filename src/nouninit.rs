use std::sync::atomic::{fence, Ordering};
use std::mem::{MaybeUninit, size_of};
use std::marker::PhantomData;
use std::ptr::NonNull;

use bytemuck::NoUninit;


/// XORs the data behind first pointer using key from second pointer.
/// The mangling operation is guaranteed to not be reordered after
/// any later operation, by usage of atomic fence with SeqCst semantics.
/// (See https://github.com/RustCrypto/utils/blob/34c554f13500dd11566922048d6e865787d6fa51/zeroize/src/lib.rs#L301-L304
/// for more details.)
/// 
/// # Safety
/// - [`data`] and [`key`] must be correctly aligned for `T`
/// - [`data`] must point to at least `size_of::<T>()` initialized bytes
///   valid for `u8` reads and writes
/// - [`key`] must point to at least `size_of::<T>()` initialized bytes
///   valid for `u8` reads
/// - [`data`] and [`key`] must either be non-overlapping or the same
unsafe fn xor_chunks<T>(data: *mut u8, key: *const u8) {
    for i in 0..size_of::<T>() {
        let data_byte = unsafe {*data.wrapping_add(i)};
        let key_byte = unsafe {*key.wrapping_add(i)};
        unsafe {
            data.wrapping_add(i).write_volatile(data_byte ^ key_byte);
        }
    }
    fence(Ordering::SeqCst);
}


/// Utility for masking a [`NoUninit`] structure in program's heap with
/// a random key.
/// Does not track ownership of the contained value if there is any,
/// and does not expose `T` nor `&T` nor `&mut T` on pain of these
/// values being potentially held on stack (perhaps in a spurious read).
///
/// This version is written using approximately reasonable amount of unsafe code,
/// at cost that it only supports [`NoUninit`] types. In particular, that
/// excludes any data with destructors; if you want those, please look at
/// [`crate::MangledBoxArbitrary`].
/// 
/// It is recommended to use [`std::clone::CloneToUninit`] to initialize
/// the contents of the box, rather than constructing it on stack.
pub struct MangledBox<T: NoUninit> {
    /// Heap allocation with bytes mangled by XORing with [`key`].
    /// Each and every byte of the pointed-to value is initialized too.
    data: Box<MaybeUninit<T>>,
    
    /// T-sized buffer containing a cryptographically secure random key.
    /// Each and every byte of the buffer is initialized.
    key: MaybeUninit<T>,
}

impl<T: NoUninit> MangledBox<T> {
    /// Constructs a new [`MangledBox`] with a random key and arbitrary data.
    pub fn new() -> Self {
        let data = Box::new_zeroed();
        // ^ [`data`] starts with arbitrary data from perspective of outer
        //   program; therefore we may choose anything, including that the block
        //   might had data equal to key (their XOR being zero).
        
        let mut key = MaybeUninit::uninit();
        getrandom::fill_uninit(key.as_bytes_mut()).expect("no keygen");
        // ^ fill_uninit guarantees that [`key`] is fully initialized on success
        
        Self {data, key}
    }

    /// Rekeys the box, preserving its contents.
    pub fn rekey(&mut self) {
        let mut diff_key = MaybeUninit::<T>::uninit();
        getrandom::fill_uninit(diff_key.as_bytes_mut()).expect("no keygen");
        
        unsafe {
            xor_chunks::<T>(Box::as_mut_ptr(&mut self.data).cast::<u8>(),
                            diff_key.as_ptr().cast::<u8>());
            xor_chunks::<T>(self.key.as_mut_ptr().cast::<u8>(),
                            diff_key.as_ptr().cast::<u8>());
        }
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
        // 2. [`data_ptr`], obtained from `&mut MaybeUninit<T>`, points
        //    to an allocation of at least `size_of::<T>()`.
        //    Our type invariant guarantees that all bytes are init too
        // 3. [`key_ptr`], obtained from `&MaybeUninit<T>`, points
        //    to an allocation of at least `size_of::<T>()`.
        //    Our type invariant guarantees that all bytes are init too
        // 4. [`data_ptr`] points to heap allocation and [`key_ptr`] to
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
                unsafe {xor_chunks::<T>(self.data, self.key)}
            }
        }
        
        // # Safety
        // 1. Both pointers point to some `MaybeUninit<T>`, so aligned
        // 2. [`data_ptr`], obtained from `&mut MaybeUninit<T>`, points
        //    to an allocation of at least `size_of::<T>()`.
        //    Our type invariant guarantees that all bytes are init too
        // 3. [`key_ptr`], obtained from `&MaybeUninit<T>`, points
        //    to an allocation of at least `size_of::<T>()`.
        //    Our type invariant guarantees that all bytes are init too
        // 4. [`data_ptr`] points to heap allocation and [`key_ptr`] to
        //    stack, therefore they do not overlap.
        let _guard = RemangleGuard::<T> {
            data: data_ptr,
            key: key_ptr,
            token: PhantomData,
        };
        
        f(data_nn.cast())
    }
}

impl<T: NoUninit> Drop for MangledBox<T> {
    fn drop(&mut self) {
        let data_ptr = Box::as_mut_ptr(&mut self.data).cast::<u8>();
        let key_ptr = self.key.as_mut_ptr().cast::<u8>();
        
        // # Safety
        // 1. Both pointers point to some `MaybeUninit<T>`, so aligned
        // 2. Both pointers were obtained from `&mut MaybeUninit<T>`
        //    to an allocation of at least `size_of::<T>()`.
        //    Our type invariant guarantees that all bytes are init too
        // 3. (2) implies that read is safe too.
        // 4. Each call passes the same pointer in both arguments.
        unsafe {
            xor_chunks::<T>(data_ptr, data_ptr);
            xor_chunks::<T>(key_ptr, key_ptr);
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn ensure_send<T: Send>(_v: &T) {}
    fn ensure_sync<T: Sync>(_v: &T) {}

    #[test]
    fn zst() {
        let mut empty_box = MangledBox::<()>::new();
        ensure_send(&empty_box);
        ensure_sync(&empty_box);

        empty_box.with_unmangled(|_| {});
    }

    #[derive(bytemuck::NoUninit, Clone, Copy)]
    #[repr(C, align(64))]
    struct Align64;

    #[test]
    fn overaligned_zst() {
        let mut align64_box = MangledBox::<Align64>::new();
        ensure_send(&align64_box);
        ensure_sync(&align64_box);

        align64_box.with_unmangled(|p| {
            assert_eq!(p.as_ptr().align_offset(64), 0,
                "alignment not preserved on overaligned ZST type");
        });
    }

    // This MangledBox depends on NoUninit trait which requires Copy.
    // Therefore, it trivially invokes no data destructors - we cannot
    // statically fit a value with Drop implementation.

    #[test]
    fn data_u8_preserved() {
        let mut box_ = MangledBox::<u8>::new();
        box_.with_unmangled(|p| {
            unsafe { p.write(42) }
        });
        box_.with_unmangled(|p| {
            assert_eq!(unsafe { p.read() }, 42);
        });
        box_.rekey();
        box_.with_unmangled(|p| {
            assert_eq!(unsafe { p.read() }, 42);
        });
        box_.with_unmangled(|p| {
            assert_eq!(unsafe { p.read() }, 42);
        });
    }

    #[test]
    fn data_u64_preserved() {
        let mut box_ = MangledBox::<u64>::new();
        let pattern: u64 = 0x123456789abcdef;

        box_.with_unmangled(|p| {
            unsafe { p.write(pattern) }
        });
        box_.with_unmangled(|p| {
            assert_eq!(unsafe { p.read() }, pattern);
        });
    }
}
