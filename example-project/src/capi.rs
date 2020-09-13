use crate::OddCounter;

// NB: The documentation comments from this file will be available
//     in the auto-generated header file example_project.h

/// Create new counter object given a start value.
///
/// On error (if an even start value is used), NULL is returned.
/// The returned object must be eventually discarded with example_project_oddcounter_free().
#[no_mangle]
pub extern "C" fn example_project_oddcounter_new(start: u32) -> Option<Box<OddCounter>> {
    OddCounter::new(start).ok().map(Box::new)
}

/// Discard a counter object.
///
/// Passing NULL is allowed.
#[no_mangle]
pub extern "C" fn example_project_oddcounter_free(_: Option<Box<OddCounter>>) {}

/// Increment a counter object.
#[no_mangle]
pub extern "C" fn example_project_oddcounter_increment(counter: &mut OddCounter) {
    counter.increment()
}

/// Obtain the current value of a counter object.
#[no_mangle]
pub extern "C" fn example_project_oddcounter_get_current(counter: &OddCounter) -> u32 {
    counter.current()
}
