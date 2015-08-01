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
use libc::{wchar_t, size_t, c_char};
pub use libc::{c_ushort, c_int};
use std::marker::PhantomData;

static mut INIT: Once = ONCE_INIT;

#[inline(always)]
unsafe fn init() {
    INIT.call_once(||{
        ffi::hid_init();
    });
}

unsafe fn wcs_to_cstring<'a>(src: *const wchar_t) -> CString {
    let length = ffi::wcstombs(std::ptr::null_mut(), src, 0);
    let mut chars = Vec::<c_char>::with_capacity(length as usize + 1);
    let ptr = chars.as_mut_ptr();
    ffi::wcstombs(ptr, src, length);
    chars[length as usize] = 0;
    CString::new(CStr::from_ptr(ptr).to_bytes_with_nul()).unwrap()
}

unsafe fn conv_hid_device_info(src_enum: *mut ffi::HidDeviceInfo) -> HidDeviceInfo {
    HidDeviceInfo {
        path: CString::new(CStr::from_ptr((*src_enum).path).to_bytes_with_nul()).unwrap(),
        vendor_id: (*src_enum).vendor_id,
        product_id: (*src_enum).product_id,
        serial_number: wcs_to_cstring((*src_enum).serial_number),
        release_number: (*src_enum).release_number,
        manufactor_string: wcs_to_cstring((*src_enum).manufactor_string),
        product_string: wcs_to_cstring((*src_enum).product_string),
        usage_page: (*src_enum).usage_page,
        usage: (*src_enum).usage,
        interface_number: (*src_enum).interface_number,
    }
}

pub struct HidDeviceInfoEnumeration {
    _hid_device_info: *mut ffi::HidDeviceInfo,
    _next: *mut ffi::HidDeviceInfo,
}

impl Drop for HidDeviceInfoEnumeration {
    fn drop(&mut self) {
        unsafe {
            ffi::hid_free_enumeration(self._hid_device_info);
        }
    }
}

impl Iterator for HidDeviceInfoEnumeration {
    type Item = HidDeviceInfo;

    fn next(&mut self) -> Option<HidDeviceInfo> {
        if self._next.is_null() {
            None
        }else {
            let ret = self._next;
            self._next = unsafe {(*self._next).next};
            Some(unsafe {conv_hid_device_info(ret)})
        }
    }
}

pub struct HidDeviceInfo {
    path: CString,
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
