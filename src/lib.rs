/****************************************************************************
    Copyright (c) 2015 Roland Ruckerbauer All Rights Reserved.

    This file is part of hidapi_rust.

    hidapi_rust is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    hidapi_rust is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with hidapi_rust.  If not, see <http://www.gnu.org/licenses/>.
****************************************************************************/

extern crate libc;

mod ffi;

use std::sync::{ONCE_INIT, Once};
use std::ffi::{CStr, CString};
pub use libc::{c_ushort, c_int, wchar_t, size_t};
use ffi::c_char;

static mut INIT: Once = ONCE_INIT;

#[inline(always)]
unsafe fn init() {
    INIT.call_once(||{
        ffi::hid_init();
    });
}

unsafe fn wcs_to_cstring(src: *const wchar_t) -> CString {
    let length = ffi::wcstombs(std::ptr::null_mut(), src, 0);
    let mut chars = Vec::<c_char>::with_capacity(length as usize + 1);
    let ptr = chars.as_mut_ptr();
    ffi::wcstombs(ptr, src, length);
    chars[length as usize] = 0;
    CString::new(chars).unwrap()
}

pub struct HidDeviceInfo<'a> {
    path: &'a CStr,
    vendor_id: c_ushort,
    product_id: c_ushort,
    serial_number: CString,
    release_number: c_ushort,
    manufactor_string: CString,
    product_string: CString,
    usage_page: c_ushort,
    usage: c_ushort,
    interface_number: c_int,
}

pub fn enumerate_hid_devices() {

}

pub struct HidDevice {
    _c_struct: *mut ffi::HidDevice,
}

impl HidDevice {

}
