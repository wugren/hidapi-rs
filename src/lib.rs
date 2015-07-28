extern crate libc;

mod ffi;

use std::sync::{ONCE_INIT, Once};

static mut INIT: Once = ONCE_INIT;

#[inline(always)]
unsafe fn init() {
    INIT.call_once(||{
        ffi::hid_init();
    });
}

pub struct HidDevice {
    _c_struct: *mut ffi::HidDevice,
}

impl HidDevice {

}
