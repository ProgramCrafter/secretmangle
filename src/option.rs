use std::ptr::{NonNull, null_mut, write};

use crate::MangledBoxArbitrary;


/// [`MangledOption`] is a variant of [`Option`] that is mangled with a random key.
/// It guarantees that value is initialized whenever [`Some`] variant is used.
///
/// [`Option`]: std::option::Option
/// [`Some`]: std::option::Option::Some
/// [`None`]: std::option::Option::None
pub enum MangledOption<T> {
    Some(MangledBoxArbitrary<T>),
    None,
}

impl<T> MangledOption<T> {
    /// Creates a new [`MangledOption`] with the [`None`] variant.
    pub fn new() -> Self {
        Self::None
    }

    /// Creates a new [`MangledOption`] with the [`Some`] variant.
    ///
    /// Please note that often you don't want to have an unmasked T value in the first place.
    /// You can construct it in-place using [`Self::insert_by_ptr`].
    pub fn filled_with_unmasked_value(value: T) -> Self {
        let mut this = Self::new();
        this.insert_unmasked_value(value);
        this
    }

    /// Returns `true` if the option is a [`Some`] variant.
    pub fn is_some(&self) -> bool {
        matches!(self, Self::Some(_))
    }

    /// Returns `true` if the option is a [`None`] variant.
    pub fn is_none(&self) -> bool {
        !self.is_some()
    }

    /// Takes the value out of the option, leaving a [`None`] in its place.
    pub fn take(&mut self) -> MangledOption<T> {
        std::mem::take(self)
    }

    /// Clears the option, dropping the value if it is a [`Some`] variant.
    pub fn clear(&mut self) {
        // Drop implementation will handle the old value
        *self = Self::None;
    }

    /// Replaces the value in the option, leaving a [`Some`] variant in its place.
    /// The old value is dropped if it was present.
    ///
    /// Please note that often you don't want to have an unmasked T value in the first place.
    /// You can construct it in-place using [`Self::insert_by_ptr`].
    pub fn insert_unmasked_value(&mut self, value: T) {
        self.insert_by_ptr(|p| unsafe { p.write(value); });
    }

    /// Replaces the value in the option, leaving a [`Some`] variant in its place.
    /// The old value is dropped if it was present, after construction of the new one.
    ///
    /// The pointer passed to the "constructor" is pointing into an uninitialized memory, allocation
    /// suitable for `T` both in size and alignment.
    pub fn insert_by_ptr(&mut self, f: impl FnOnce(NonNull<T>)) {
        let mut new_content_box = MangledBoxArbitrary::new();
        new_content_box.with_unmangled(f);
        *self = Self::Some(new_content_box);
    }

    /// Unmangles the contents and invokes the provided closure on it. Invokes a default
    /// closure if the option is [`None`] instead.
    ///
    /// An immutable version is not available because it would still need to make a mutation, to
    /// read the data, and it is not possible to do concurrently.
    ///
    /// Please check the compiled code to determine if your function makes a spurious copy
    /// which could be a security issue.
    pub fn map_mut_or_else<F, G, R>(&mut self, default: G, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
        G: FnOnce() -> R,
    {
        match self {
            MangledOption::Some(mangled_box) => {
                mangled_box.with_unmangled(|mut ptr| f(unsafe { ptr.as_mut() }))
            }
            MangledOption::None => default(),
        }
    }

    /// Unmangles the contents and invokes the provided closure on it.
    ///
    /// An immutable version is not available because it would still need to make a mutation, to
    /// read the data, and it is not possible to do concurrently.
    ///
    /// Please check the compiled code to determine if your function makes a spurious copy
    /// which could be a security issue.
    pub fn map_mut<F, R>(&mut self, f: F) -> Option<R>
    where
        F: FnOnce(&mut T) -> R,
    {
        self.map_mut_or_else(|| None, |m| Some(f(m)))
    }

    /// Rekeys the box, preserving its contents.
    pub fn rekey(&mut self) {
        match self {
            MangledOption::Some(mangled_box) => {
                mangled_box.rekey();
            }
            MangledOption::None => {}
        }
    }

    /// Returns pointer to mangled data.
    pub fn as_ptr(&mut self) -> *mut T {
        match self {
            MangledOption::Some(mangled_box) => mangled_box.with_mangled(|p| p.as_ptr()),
            MangledOption::None              => null_mut(),
        }
    }
}

impl<T> Drop for MangledOption<T> {
    fn drop(&mut self) {
        match self {
            MangledOption::Some(mangled_box) => {
                unsafe { mangled_box.drop_in_place(); }
            }
            MangledOption::None => {}
        }
        unsafe { write(self as *mut Self, Self::None); }
    }
}

impl<T> Default for MangledOption<T> {
    fn default() -> Self {
        Self::None
    }
}


#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::mem::size_of;

    use super::*;


    #[test]
    fn test_map_mut() {
        let mut option = MangledOption::filled_with_unmasked_value(42);
        assert_eq!(option.map_mut(|x| { *x += 1; *x }), Some(43));
    }

    #[test]
    fn test_map_mut_or_else() {
        let mut option = MangledOption::filled_with_unmasked_value(42);
        assert_eq!(option.map_mut_or_else(|| 5, |x| { *x += 1; *x }), 43);

        option = MangledOption::None;
        assert_eq!(option.map_mut_or_else(|| 5, |x| { *x += 1; *x }), 5);
    }
    
    #[test]
    fn test_new_is_none() {
        let option: MangledOption<i32> = MangledOption::new();
        assert!(option.is_none());
    }

    #[test]
    fn test_filled_with_unmasked_value() {
        let mut option = MangledOption::filled_with_unmasked_value(10);
        assert!(option.is_some());
        assert_eq!(option.map_mut(|x| *x), Some(10));
    }

    #[test]
    fn test_take() {
        let mut option = MangledOption::filled_with_unmasked_value(20);
        let mut taken = option.take();
        
        assert!(option.is_none());
        assert_eq!(taken.map_mut(|x| *x), Some(20));
    }

    #[test]
    fn test_clear() {
        let mut option = MangledOption::filled_with_unmasked_value(30);
        option.clear();
        assert!(option.is_none());
    }

    #[test]
    fn test_insert_unmasked_value() {
        let mut option = MangledOption::new();
        option.insert_unmasked_value(40);
        assert_eq!(option.map_mut(|x| *x), Some(40));
        
        option.insert_unmasked_value(50); // Replace existing
        assert_eq!(option.map_mut(|x| *x), Some(50));
    }

    #[test]
    fn test_insert_by_ptr() {
        let mut option = MangledOption::<usize>::new();
        option.insert_by_ptr(|ptr| unsafe { ptr.as_ptr().write(60) });
        assert_eq!(option.map_mut(|x| *x), Some(60));
        
        // `*ptr.as_ptr() = 70` would be UB because of touching uninit bytes
        option.insert_by_ptr(|ptr| unsafe { ptr.as_ptr().write(70) });
        assert_eq!(option.map_mut(|x| *x), Some(70));
    }

    #[test]
    fn test_rekey() {
        let mut option = MangledOption::filled_with_unmasked_value(80);
        let original_value = option.map_mut(|x| *x).unwrap();
        
        option.rekey(); // Should preserve value
        assert_eq!(option.map_mut(|x| *x), Some(original_value));
    }

    #[test]
    fn test_drop_behavior() {
        static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

        struct DropCounter;
        impl Drop for DropCounter {
            fn drop(&mut self) {
                DROP_COUNT.fetch_add(1, Ordering::SeqCst);
            }
        }

        {
            let _option = MangledOption::filled_with_unmasked_value(DropCounter);
            assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0);
        }
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);

        {
            let _option = MangledOption::<DropCounter>::new();
            assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);
        }
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);

        {
            let mut option = MangledOption::filled_with_unmasked_value(DropCounter);
            assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);
            option.clear();
            assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 2);
        }
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 2);

        {
            let mut option = MangledOption::new();
            assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 2);
            option.insert_unmasked_value(DropCounter);
        }
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 3);
    }
    
    #[test]
    fn test_string_content() {
        let s = String::from("test_value");
        let mut option = MangledOption::filled_with_unmasked_value(s);
        
        option.map_mut(|inner| assert_eq!(inner, "test_value"));
        
        option.map_mut(|inner| inner.push_str("_modified"));
        option.map_mut(|inner| assert_eq!(inner, "test_value_modified"));
        
        // Rekey and verify integrity
        option.rekey();
        option.map_mut(|inner| assert_eq!(inner, "test_value_modified"));
    }

    #[test]
    fn test_padded_struct() {
        #[repr(C)]
        #[derive(Debug, PartialEq)]
        struct Padded {
            a: u8,
            b: u16,
            c: u32,
        }

        let val = Padded { a: 0xAA, b: 0xBBBB, c: 0xCCCCCCCC };
        let mut option = MangledOption::filled_with_unmasked_value(val);
        
        option.map_mut(|inner| assert_eq!(*inner, Padded { a: 0xAA, b: 0xBBBB, c: 0xCCCCCCCC }));
        
        option.map_mut(|inner| inner.a = 0x11);
        option.map_mut(|inner| {
            assert_eq!(inner.a, 0x11);
            assert_eq!(inner.b, 0xBBBB);
            assert_eq!(inner.c, 0xCCCCCCCC);
        });
        
        // Rekey and verify integrity
        option.rekey();
        option.map_mut(|inner| {
            inner.b = 0x2222;
            assert_eq!(inner.a, 0x11);
        });
        option.map_mut(|inner| assert_eq!(*inner, Padded { a: 0x11, b: 0x2222, c: 0xCCCCCCCC }));
    }

    #[test]
    fn test_large_inline_struct() {
        #[derive(PartialEq, Eq, Debug)]
        struct LargeStruct([u64; 8]);
        
        let val = LargeStruct([0xDEADBEEF; 8]);
        let mut option = MangledOption::filled_with_unmasked_value(val);
        
        option.map_mut(|inner| assert_eq!(inner.0, [0xDEADBEEF; 8]));
        
        // Modify and verify
        option.map_mut(|inner| inner.0[4] = 0xCAFEBABE);
        option.map_mut(|inner| {
            assert_eq!(inner.0[0], 0xDEADBEEF);
            assert_eq!(inner.0[4], 0xCAFEBABE);
        });
    }

    #[test]
    fn test_rekey_integrity() {
        struct Nested {
            a: u32,
            b: MangledOption<u64>,
        }
        
        let mut option = MangledOption::filled_with_unmasked_value(Nested {
            a: 0x12345678,
            b: MangledOption::filled_with_unmasked_value(0xABCDEF),
        });
        
        option.map_mut(|inner| {
            assert_eq!(inner.a, 0x12345678);
            assert_eq!(inner.b.map_mut(|x| *x), Some(0xABCDEF));
        });
        
        // Rekey outer and inner
        option.rekey();
        option.map_mut(|inner| {
            inner.b.rekey();
            inner.a = 0x87654321;
        });
        
        option.map_mut(|inner| {
            assert_eq!(inner.a, 0x87654321);
            assert_eq!(inner.b.map_mut(|x| *x), Some(0xABCDEF));
        });
        option.map_mut(|inner| {
            inner.b.map_mut(|x| *x = 0x123456789);
        });
        
        // Final verification
        option.map_mut(|inner| {
            assert_eq!(inner.b.map_mut(|x| *x), Some(0x123456789));
        });
    }
    
    #[test]
    fn xor_behavior() {
        #[repr(C)]
        #[derive(Debug, PartialEq)]
        struct Padded {
            a: u8,
            b: u16,
            c: u32,
        }
        
        let mut option = MangledOption::new();
        option.insert_by_ptr(|ptr: NonNull<Padded>| {
            let a = ptr.addr().trailing_zeros() as u8;
            let c = size_of::<Padded>() as u32;
            
            // Constructing by parts.
            unsafe {
                let place: *mut u8 = ptr.as_ptr().cast();
                place.write(a);
                place.add(2).write(0xAA);
                place.add(3).write(0xBB);
                place.add(4).cast::<u32>().write(c);
            }
        });
        
        let had = option.map_mut(|inner| {
            assert!(inner.a < 64);
            assert_eq!(inner.b, u16::from_ne_bytes([0xAA, 0xBB]));
            assert_eq!(inner.c as usize, size_of::<Padded>());
        });
        assert!(had.is_some());
        
        let p: *mut u8 = option.as_ptr().cast();
        unsafe {
            p.write(p.read() ^ 128);
        }
        let had = option.map_mut(|inner| {
            assert!(inner.a > 128);
            assert_eq!(inner.b, u16::from_ne_bytes([0xAA, 0xBB]));
            assert_eq!(inner.c as usize, size_of::<Padded>());
        });
        assert!(had.is_some());
    }
}

