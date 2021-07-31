use crate::add_two;

// NB: The documentation comments from this file will be available
//     in the auto-generated header file.

/// Adds two to the given value and returns the result.
#[no_mangle]
pub extern "C" fn example_project_with_subdir_add_two(value: u32) -> u32 {
    add_two(value)
}
