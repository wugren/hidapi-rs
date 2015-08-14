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

use std::ffi::{CStr};
use libc::{wchar_t, c_char, size_t};
pub use libc::{c_ushort, c_int};

pub struct HidApi;

static mut hid_api_lock: bool = false;

impl HidApi {
    pub fn new() -> Result<Self, &'static str> {
        if unsafe {!hid_api_lock} {
            unsafe {
                ffi::hid_init();
                hid_api_lock = true;
            }
            Ok(HidApi)
        }else {
            Err("Error already one HidApi in use.")
        }
    }

    pub fn enumerate_info(&self) -> HidDeviceInfoEnumeration {
        let list = unsafe {ffi::hid_enumerate(0, 0)};
        HidDeviceInfoEnumeration {
            _hid_device_info: list,
            _next: list,
        }
    }

    pub fn open(&self, vendor_id: c_ushort, product_id: c_ushort)
            -> Result<HidDevice, &'static str> {
        let device = unsafe {ffi::hid_open(vendor_id, product_id, std::ptr::null())};
        if device.is_null() {
            Err("Can not open hid device.")
        }else {
            Ok(HidDevice {_hid_device: device, api: self})
        }
    }
}

impl Drop for HidApi {
    fn drop(&mut self) {
        unsafe {
            ffi::hid_exit();
            hid_api_lock = false;
        }
    }
}

unsafe fn wcs_to_string<'a>(src: *const wchar_t) -> String {
    let length = ffi::wcstombs(std::ptr::null_mut(), src, 0);
    let mut chars = Vec::<c_char>::with_capacity(length as usize + 1);
    chars.set_len(length as usize + 1);
    let ptr = chars.as_mut_ptr();
    ffi::wcstombs(ptr, src, length);
    chars[length as usize] = 0;
    std::str::from_utf8(CStr::from_ptr(ptr).to_bytes()).unwrap().to_owned()
}

unsafe fn conv_hid_device_info(src: *mut ffi::HidDeviceInfo) -> HidDeviceInfo {
    HidDeviceInfo {
        path: std::str::from_utf8(CStr::from_ptr((*src).path).to_bytes()).unwrap().to_owned(),
        vendor_id: (*src).vendor_id,
        product_id: (*src).product_id,
        //serial_number: wcs_to_string((*src).serial_number),
        release_number: (*src).release_number,
        manufactor_string: wcs_to_string((*src).manufactor_string),
        product_string: wcs_to_string((*src).product_string),
        usage_page: (*src).usage_page,
        usage: (*src).usage,
        interface_number: (*src).interface_number,
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

#[derive(Debug)]
pub struct HidDeviceInfo {
    path: String,
    vendor_id: c_ushort,
    product_id: c_ushort,
    //serial_number: String,
    release_number: c_ushort,
    manufactor_string: String,
    product_string: String,
    usage_page: c_ushort,
    usage: c_ushort,
    interface_number: c_int,
}

pub struct HidDevice<'a> {
    _hid_device: *mut ffi::HidDevice,
    #[allow(dead_code)]
    api: &'a HidApi, // Just to keep everything safe.
}

impl<'a> Drop for HidDevice<'a> {
    fn drop(&mut self) {
        unsafe {ffi::hid_close(self._hid_device)};
    }
}

impl <'a> HidDevice<'a> {
    pub fn write(&self, data: &[u8]) -> c_int {
        unsafe {ffi::hid_write(self._hid_device, data.as_ptr(), data.len() as size_t)}
    }

    pub fn send_feature_report(&self, data: &[u8]) -> c_int {
        unsafe {
            ffi::hid_send_feature_report(self._hid_device, data.as_ptr(), data.len() as size_t)
        }
    }
}
