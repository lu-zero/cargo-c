use std::ffi::CStr;

use libc::c_char;

#[no_mangle]
pub unsafe extern "C" fn example_project_hello(name: *const c_char) {
    match CStr::from_ptr(name).to_str() {
        Ok(name) => crate::hello(name),
        Err(e) => eprintln!("Error: invalid name: {}", e),
    }
}
