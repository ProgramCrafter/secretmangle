#![feature(maybe_uninit_as_bytes, box_vec_non_null, new_zeroed_alloc, box_as_ptr)]
#![feature(clone_to_uninit)]

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
pub use arbitrary::MangledBoxArbitrary;
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
mod arbitrary;

pub use nouninit::MangledBox;
mod nouninit;
