//! Example library for [cargo-c].
//!
//! [cargo-c]: https://crates.io/crates/cargo-c

#![warn(rust_2018_idioms)]
#![deny(missing_docs)]

#[cfg(feature = "capi")]
mod capi;

/// Adds two to the given value and returns the result.
pub fn add_two(value: u32) -> u32 {
    value + 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_two_to_fourty() {
        assert_eq!(add_two(40), 42);
    }
}
