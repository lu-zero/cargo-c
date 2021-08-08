//! Example library for [cargo-c].
//!
//! [cargo-c]: https://crates.io/crates/cargo-c

#![deny(missing_docs)]

#[cfg(feature = "capi")]
mod capi;

/// Adds two to the given value and returns the result.
pub fn add_two(value: usize) -> usize {
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
