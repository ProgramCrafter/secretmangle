#![feature(maybe_uninit_as_bytes, box_vec_non_null, new_zeroed_alloc, box_as_ptr)]
#![feature(clone_to_uninit)]


mod arbitrary;
mod nouninit;

pub use nouninit::MangledBox;
pub use arbitrary::MangledBoxArbitrary;
