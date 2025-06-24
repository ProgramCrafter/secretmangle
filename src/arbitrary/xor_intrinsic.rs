//! In C, C++, Rust and whatever languages, it is Undefined Behavior to read
//! padding bytes of a struct, because those are generally not initialized.
//! We cannot know which bytes are padding and which are data in advance (nor
//! compile- nor runtime) so we have to mask all of them.
//! 
//! That necessitates assembly code.


/// XORs the data behind the first pointer using the key from the second pointer
/// in a fashion that does not provide ordering guarantees but is guaranteed
/// not to be elided.
/// 
/// # Safety
/// - [`data`] and [`key`] must be correctly aligned for `T`
/// - [`data`] and [`key`] must have at least `size_of::<T>()` bytes allocated
/// - [`data`] and [`key`] must either be non-overlapping or the same
///
/// No requirements on initialization status are made.
/// Garbage in, garbage out - instead of UB out.
#[cfg(target_arch = "x86_64")]
pub unsafe fn xor_chunks_intrinsic_baseline<T>(data: *mut u8, key: *const u8) {
    let size = std::mem::size_of::<T>();
    let min_alignment = std::mem::align_of::<T>();
    let min_alignment_bits: u32 = min_alignment.trailing_zeros();
    
    let co_aligned_bits = data.addr().trailing_zeros()
        .min(key.addr().trailing_zeros());
    debug_assert!(co_aligned_bits >= min_alignment_bits,
        "first safety precondition: data and key must be aligned for T");
    
    let index = 0usize;
    unsafe {
        // TODO: consider wider-sized loads
        // TODO: consider partial loop unrolling
        std::arch::asm!(
            "2:",
                "cmp {index}, {size}",
                "jae 3f",
                "mov {key_byte}, byte ptr [{key} + {index}]",
                "xor byte ptr [{data} + {index}], {key_byte}",
                "add {index}, 1",
                "jmp 2b",
            "3:",
            index = inout(reg) index => _,
            size = in(reg) size,
            data = in(reg) data,
            key = in(reg) key,
            key_byte = out(reg_byte) _,
            options(nostack),
        );
    }
}

#[cfg(target_arch = "aarch64")]
pub unsafe fn xor_chunks_intrinsic_baseline<T>(data: *mut u8, key: *const u8) {
    use std::arch::asm;
    
    let size = std::mem::size_of::<T>();
    let min_alignment = std::mem::align_of::<T>();
    let min_alignment_bits: u32 = min_alignment.trailing_zeros();
    
    let co_aligned_bits = data.addr().trailing_zeros()
        .min(key.addr().trailing_zeros());
    debug_assert!(co_aligned_bits >= min_alignment_bits,
        "first safety precondition: data and key must be aligned for T");
    
    let mut index = 0usize;
    
    unsafe {
        asm!(
            "b 2f",
            "1:",
                "ldrb {key_byte}, [{key}, {index}]",
                "ldrb {tmp}, [{data}, {index}]",
                "eor {tmp}, {tmp}, {key_byte}",
                "strb {tmp}, [{data}, {index}]",
                "add {index}, {index}, #1",
            "2:",
                "cmp {index}, {size}",
                "b.lo 1b",
            key_byte = out(reg_byte) _,
            tmp = out(reg) _,
            index = inout(reg) index,
            size = in(reg) size,
            data = in(reg) data,
            key = in(reg) key,
            options(nostack),
        );
    }
}


#[cfg(all(test, not(miri)))]
mod tests {
    use super::*;

    #[derive(Default)]
    #[repr(C)]
    struct Foo {
        a: u8,
        b: u16,
    }

    #[expect(dead_code)]
    #[derive(Default)]
    #[repr(align(16))]
    struct Align16 {
        a: u64,
        b: u8,
        c: u64,
    }

    fn test_xor_chunks_for_type<T: Default>() {
        let mut data = T::default();
        let mut key = T::default();
        let size = std::mem::size_of::<T>();

        let data_ptr = (&raw mut data).cast::<u8>();
        let key_ptr = (&raw mut key).cast::<u8>();
        
        unsafe {
            // Initialize data to 0xAA and key to 0x55
            std::ptr::write_bytes(data_ptr, 0xAA, size);
            std::ptr::write_bytes(key_ptr, 0x55, size);
            
            // XOR data with key
            xor_chunks_intrinsic_baseline::<T>(data_ptr, key_ptr);
            
            // Verify each byte is 0xAA ^ 0x55 = 0xFF
            for i in 0..size {
                assert_eq!(data_ptr.add(i).read(), 0xFF);
            }
            
            // XOR again with the same key to revert
            xor_chunks_intrinsic_baseline::<T>(data_ptr, key_ptr);
            
            // Verify back to 0xAA
            for i in 0..size {
                assert_eq!(data_ptr.add(i).read(), 0xAA);
            }
            
            // Test with the same pointer (data XOR data)
            xor_chunks_intrinsic_baseline::<T>(data_ptr, data_ptr);
            
            // Verify all zeros
            for i in 0..size {
                assert_eq!(data_ptr.add(i).read(), 0);
            }
            
            data_ptr.cast::<T>().write(T::default());
            key_ptr.cast::<T>().write(T::default());
        }
    }

    #[test]
    fn test_bytewise() {
        test_xor_chunks_for_type::<u8>();
        test_xor_chunks_for_type::<u16>();
        test_xor_chunks_for_type::<u32>();
        test_xor_chunks_for_type::<u64>();
        test_xor_chunks_for_type::<Foo>();
        test_xor_chunks_for_type::<Align16>();
    }

    #[test]
    fn test_offsetted() {
        let mut data: [u16; 256] = std::array::from_fn(|i| i as u16);
        let mut manual_data = data.clone();
        let key = [248, 230, 123, 176, 35, 3, 156, 13, 204, 19, 196, 124, 160,
            184, 59, 232, 107, 98, 197, 117, 61, 97, 94, 172, 155, 68, 182, 72,
            5, 108, 221, 228, 142, 114, 58, 211, 41, 21, 22, 168, 169, 189, 158,
            52, 183, 136, 171, 56, 50, 223, 207, 226, 175, 144, 205, 234, 254,
            40, 251, 9, 148, 213, 238, 30, 163, 16, 209, 55, 135, 244, 11, 212,
            194, 216, 29, 233, 60, 153, 26, 141, 146, 152, 7, 210, 64, 36, 191,
            147, 180, 208, 243, 104, 165, 89, 224, 10, 125, 24, 131, 6, 115,
            38, 195, 187, 70, 231, 198, 130, 78, 80, 139, 229, 250, 214, 154,
            63, 54, 113, 120, 76, 67, 242, 235, 77, 48, 88, 225, 105, 170, 166,
            20, 0, 134, 82, 57, 86, 102, 109, 25, 133, 239, 37, 157, 245, 137,
            85, 53, 111, 192, 174, 218, 185, 240, 203, 96, 101, 12, 51, 201,
            110, 143, 116, 150, 119, 2, 140, 186, 66, 83, 39, 18, 188, 252,
            237, 199, 118, 69, 215, 255, 93, 247, 132, 45, 49, 217, 99, 4, 84,
            90, 100, 121, 126, 128, 75, 177, 8, 42, 246, 28, 202, 74, 32, 31,
            81, 23, 167, 151, 220, 193, 178, 14, 241, 138, 219, 190, 103, 179,
            122, 79, 129, 44, 112, 46, 1, 95, 222, 91, 162, 73, 127, 33, 145,
            27, 71, 249, 253, 92, 34, 47, 15, 173, 161, 62, 149, 227, 181, 236,
            106, 206, 200, 159, 43, 87, 164, 65, 17_u16];
        
        fn test<S>(
            data: &mut [u16; 256],
            manual_data: &mut [u16; 256],
            key: &[u16; 256],
            d: usize,
            k: usize,
        ) {
            let s = std::mem::size_of::<S>();
            let mult = std::mem::align_of::<u16>();
            debug_assert!(d * mult + s <= data.len() * mult);
            debug_assert!(k * mult + s <= key.len() * mult);

            unsafe {
                let data_ptr = data.as_mut_ptr().add(d).cast::<u8>();
                let key_ptr = key.as_ptr().add(k).cast::<u8>();
                xor_chunks_intrinsic_baseline::<S>(data_ptr, key_ptr);
            }

            for i in 0..s/mult {
                manual_data[d + i] ^= key[k + i];
            }

            assert_eq!(data, manual_data);
        }

        test::<[u8; 38]>(&mut data, &mut manual_data, &key, 0, 0);
        test::<[u8; 24]>(&mut data, &mut manual_data, &key, 0, 0);
        test::<[u8; 24]>(&mut data, &mut manual_data, &key, 0, 16);
        test::<[u8; 24]>(&mut data, &mut manual_data, &key, 3, 0);
        test::<[u16; 24]>(&mut data, &mut manual_data, &key, 4, 0);
        test::<[u16; 24]>(&mut data, &mut manual_data, &key, 4, 40);
        test::<[u64; 9]>(&mut data, &mut manual_data, &key, 8, 0);
        test::<[u16; 215]>(&mut data, &mut manual_data, &key, 40, 0);
    }

    #[test]
    fn test_structurewise() {
        // Test with a simple type (no padding)
        let mut data = [0xAAu8, 0xBB];
        let key = [0xFFu8, 0xEE];
        unsafe {
            xor_chunks_intrinsic_baseline::<[u8; 2]>(data.as_mut_ptr(), key.as_ptr());
        }
        assert_eq!(data, [0xAA ^ 0xFF, 0xBB ^ 0xEE]);

        // Test with a struct that has padding
        #[derive(PartialEq, Eq, Debug)]
        #[repr(C)]
        struct Padded {
            a: u8,
            b: u32,
        }
        let mut data = Padded { a: 0x12, b: 0x3456789A };
        let key = vec![0xFF, 0x00, 0x00, 0x00, 0xEE, 0xDD, 0xCC, 0xBB];
        unsafe {
            xor_chunks_intrinsic_baseline::<Padded>(
                (&raw mut data).cast::<u8>(),
                key.as_ptr(),
            );
        }
        assert_eq!(data.a, 0x12 ^ 0xFF);
        assert_eq!(data.b, 0x3456789A ^ 0xEEDDCCBB_u32.swap_bytes());
        unsafe {
            xor_chunks_intrinsic_baseline::<[u8; 8]>(
                (&raw mut data).cast::<u8>(),
                key.as_ptr(),
            );
        }
        assert_eq!(data, Padded { a: 0x12, b: 0x3456789A });
    }
}