//!  Example library for [cargo-c].
//!
//! [cargo-c]: https://crates.io/crates/cargo-c

#![warn(rust_2018_idioms)]
#![deny(missing_docs)]

#[cfg(feature = "capi")]
mod capi;

/// A counter for odd numbers.
///
/// Note that this `struct` does *not* use `#[repr(C)]`.
/// It can therefore contain arbitrary Rust types.
/// In the C API, it will be available as an *opaque pointer*.
#[derive(Debug)]
pub struct OddCounter {
    number: u32,
}

impl OddCounter {
    /// Create a new counter, given an odd number to start.
    pub fn new(start: u32) -> Result<OddCounter, OddCounterError> {
        if start % 2 == 0 {
            Err(OddCounterError::Even)
        } else {
            Ok(OddCounter { number: start })
        }
    }

    /// Increment by 2.
    pub fn increment(&mut self) {
        self.number += 2;
    }

    /// Obtain the current (odd) number.
    pub fn current(&self) -> u32 {
        self.number
    }
}

/// Error type for [OddCounter::new].
///
/// In a "real" library, there would probably be more error variants.
#[derive(Debug)]
pub enum OddCounterError {
    /// An even number was specified as `start` value.
    Even,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create42() {
        assert!(OddCounter::new(42).is_err());
    }

    #[test]
    fn increment43() {
        let mut counter = OddCounter::new(43).unwrap();
        counter.increment();
        assert_eq!(counter.current(), 45);
    }
}
