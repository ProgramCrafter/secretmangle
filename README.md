# secretmangle

![Unsafety Amount](https://img.shields.io/badge/unsafe-100%25-red)
[![Documentation](https://docs.rs/secretmangle/badge.svg)](https://docs.rs/secretmangle)

A Rust library for managing sensitive structures by XORing them with cryptographically secure random keys, which are held separately from the data allocation. This helps protect against partial memory disclosure attacks, and also from cursory memory inspection.

## Features

- **UB-free** library (in all probability)
    - on `nouninit` side, Miri succeeds
    - on `arbitrary` side, Miri cannot test inline assembly but the intrinsic is straightforward + hardware does not have uninit memory semantics
    - if something fails, I would like to know about it
- **Zero-sized types (ZST)** support
- **Over-aligned types** support

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
secretmangle = "0.1.0"
```

### Basic Example

```rust
use secretmangle::{MangledBox, MangledBoxArbitrary};

// For types that implement NoUninit (no padding, Copy)
let mut secret = MangledBox::new(42u32);
secret.with_unmangled(|value| {
    assert_eq!(*value, 42);
    *value += 1;
});

// For arbitrary types (including those with Drop)
let mut secret_string = MangledBoxArbitrary::<String>::new();
secret_string.with_unmangled(|s| {
    s.push_str("Hello, world!");
});
// Please note this does not mask the string's characters, but only its
// controlling allocation `String` (that is, three pointer-sized values).

unsafe {
    secret_string.drop_in_place();
}
// When you are done with the box, drop its contents if you are certain
// that it was initialized. `MangledBox` remains operational if you need it.
```

## How It Works

The crate provides two main types:

1. `MangledBox<T: bytemuck::NoUninit>` - For types that are `Copy` and have no padding
2. `MangledBoxArbitrary<T>` - For any type, including those with destructors

Both types store the data XORed with a random key. The data is only decrypted when accessed through the `with_unmangled` method, and it's immediately re-encrypted when the closure returns or panics.

The wrappers do not expose `T` nor `&T` nor `&mut T` on pain of leaving copies of inner data somewhere on stack. To guard data from being disclosed after usage, an atomic fence separates the mangling from subsequent code (parallel to [zeroize](https://github.com/RustCrypto/utils/blob/34c554f13500dd11566922048d6e865787d6fa51/zeroize/src/lib.rs#L301-L304)'s implementation).

## License

Licensed under either of:

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.