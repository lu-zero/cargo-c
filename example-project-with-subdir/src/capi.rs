use libc::size_t;

use crate::add_two;

// NB: The documentation comments from this file will be available
//     in the auto-generated header file.

/// Adds two to the given value and returns the result.
#[no_mangle]
pub extern "C" fn example_project_with_subdir_add_two(value: size_t) -> size_t {
    add_two(value)
}
