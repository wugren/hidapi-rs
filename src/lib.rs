/****************************************************************************
    Copyright (c) 2015 Osspial All Rights Reserved.

    This file is part of hidapi-rs, based on hidapi_rust by Roland Ruckerbauer.

    hidapi-rs is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    hidapi-rs is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with hidapi-rs.  If not, see <http://www.gnu.org/licenses/>.
****************************************************************************/

extern crate libc;

mod ffi;

use std::ffi::{CStr};
use std::marker::PhantomData;
use libc::{wchar_t, size_t};
pub use libc::{c_ushort, c_int};

pub type HidError = &'static str;
pub type HidResult<T> = Result<T, HidError>;
const STRING_BUF_LEN: usize = 128;

pub struct HidApi {
    devices: Vec<HidDeviceInfo>,
}

static mut hid_api_lock: bool = false;

impl HidApi {
    ///Initializes the HID
    pub fn new() -> HidResult<Self> {
        if unsafe {!hid_api_lock} {

            //Initialize the HID and prevent other HIDs from being created
            unsafe {
                if ffi::hid_init() == -1 {
                    return Err("Failed to init hid");
                }
                hid_api_lock = true;
            }


            Ok(HidApi{devices: unsafe {HidApi::get_hid_device_info_vector()}})

        } else {
            Err("HidApi already in use")
        }
    }

    ///Refresh devices list
    pub fn refresh_devices(&mut self) {
        self.devices = unsafe {HidApi::get_hid_device_info_vector()};
    }

    unsafe fn get_hid_device_info_vector() -> Vec<HidDeviceInfo> {
        let mut device_vector = Vec::with_capacity(8);

        let enumeration = ffi::hid_enumerate(0, 0);
        {
            let mut current_device = enumeration;

            'do_while: loop {

                device_vector.push(conv_hid_device_info(current_device));

                if (*current_device).next.is_null() {
                    break 'do_while;
                } else {
                    current_device = (*current_device).next;
                }
            }
        }

        ffi::hid_free_enumeration(enumeration);

        device_vector
    }

    ///Returns list of objects containing information about connected devices
    pub fn devices(&self) -> Vec<HidDeviceInfo> {
        self.devices.clone()
    } 

    ///Open a HID device using a Vendor ID (VID) and Product ID (PID)
    pub fn open(&self, vid: u16, pid: u16) -> HidResult<HidDevice> {
        let device = unsafe {ffi::hid_open(vid, pid, std::ptr::null())};

        if device.is_null() {
            Err("Can't open hid device")
        } else {
            Ok(HidDevice {_hid_device: device, phantom: PhantomData})
        }
    }

    ///Open a HID device using a Vendor ID (VID), Product ID (PID) and
    ///a serial number.
    pub fn open_serial(&self, vid: u16, pid: u16, sn: &str) -> HidResult<HidDevice> {
        let device = unsafe {ffi::hid_open(vid, pid,
            std::mem::transmute(sn.as_ptr()))};
        if device.is_null() {
            Err("Can't open hid device")
        } else {
            Ok(HidDevice {_hid_device: device, phantom: PhantomData})
        }
    }

    ///The path name be determined by calling hid_enumerate(), or a
    ///platform-specific path name can be used (eg: /dev/hidraw0 on Linux).
    pub fn open_path(&self, device_path: &str) -> HidResult<HidDevice> {
        let device = unsafe {ffi::hid_open_path(
            std::mem::transmute(device_path.as_ptr()))};

        if device.is_null() {
            Err("Cannot open hid device")
        } else {
            Ok(HidDevice {_hid_device: device, phantom: PhantomData})
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

///Converts a pointer to a `wchar_t` to a string
unsafe fn wchar_to_string(wstr: *const wchar_t) -> HidResult<String> {

    if wstr.is_null() {
        return Err("Null pointer!");
    }

    let mut char_vector: Vec<char> = Vec::with_capacity(8);
    let mut index: isize = 0;

    while *wstr.offset(index) != 0 {
        use std::char;
        char_vector.push(match char::from_u32(*wstr.offset(index) as u32) {
            Some(ch) => ch,
            None => return Err("Unable to add next char")
        });

        index += 1;
    }

    Ok(char_vector.into_iter().collect())
}

///Convert the CFFI `HidDeviceInfo` struct to a native `HidDeviceInfo` struct
unsafe fn conv_hid_device_info(src: *mut ffi::HidDeviceInfo) -> HidDeviceInfo {
    HidDeviceInfo {
        path: std::str::from_utf8(CStr::from_ptr((*src).path).to_bytes()).unwrap().to_owned(),
        vendor_id: (*src).vendor_id,
        product_id: (*src).product_id,
        serial_number: wchar_to_string((*src).serial_number).ok(),
        release_number: (*src).release_number,
        manufacturer_string: wchar_to_string((*src).manufacturer_string).ok(),
        product_string: wchar_to_string((*src).product_string).ok(),
        usage_page: (*src).usage_page,
        usage: (*src).usage,
        interface_number: (*src).interface_number,
    }
}

#[derive(Debug, Clone)]
pub struct HidDeviceInfo {
    path: String,
    vendor_id: u16,
    product_id: u16,
    serial_number: Option<String>,
    release_number: u16,
    manufacturer_string: Option<String>,
    product_string: Option<String>,
    usage_page: u16,
    usage: u16,
    interface_number: i32,
}

impl HidDeviceInfo {
    /// Platform-specific device path
    pub fn get_path(&self) -> String {
        self.path.clone()
    }

    pub fn get_vendor_id(&self) -> u16 {
        self.vendor_id
    }

    pub fn get_product_id(&self) -> u16 {
        self.product_id
    }

    pub fn get_serial_number(&self) -> Option<String> {
        self.serial_number.clone()
    }

    pub fn get_release_number(&self) -> u16 {
        self.release_number
    }

    pub fn get_manufacturer_string(&self) -> Option<String> {
        self.manufacturer_string.clone()
    }

    pub fn get_product_string(&self) -> Option<String> {
        self.product_string.clone()
    }

    pub fn get_usage_page(&self) -> u16 {
        self.usage_page
    }

    pub fn get_usage(&self) -> u16 {
        self.usage
    }

    pub fn get_interface_number(&self) -> i32 {
        self.interface_number
    }
}

///Object for accessing HID device
pub struct HidDevice<'a> {
    _hid_device: *mut ffi::HidDevice,
    /// Prevents this from outliving the api instance that created it
    phantom: PhantomData<&'a ()>
}

impl<'a> Drop for HidDevice<'a> {
    fn drop(&mut self) {
        unsafe {ffi::hid_close(self._hid_device)};
    }
}

impl <'a> HidDevice<'a> {
    fn check_size(&self, res: i32) -> HidResult<usize> {
        if res == -1 {
            match self.check_error() {
                Ok(err) => {
                    if err.is_empty() {
                        Err("Undetected error")    
                    } else {
                        println!("{:?}", err);
                        Err("Detected error")
                    }
                },
                Err(_) => {
                    //Err(err)
                    Err("Failed to decode error message")
                }
            }
        } else {
            Ok(res as usize)
        }
    }

    pub fn check_error(&self) -> HidResult<String> {
        unsafe {wchar_to_string(ffi::hid_error(self._hid_device))}
    }

    pub fn write(&self, data: &[u8]) -> HidResult<usize> {
        let res = unsafe {ffi::hid_write(self._hid_device,
            data.as_ptr(), data.len() as size_t)};
        self.check_size(res)
    }

    pub fn read<'b>(&mut self, buf: &'b mut [u8]) -> HidResult<&'b [u8]> {
        let res = unsafe {ffi::hid_read(self._hid_device,
            buf.as_mut_ptr(), buf.len() as size_t)};
        let res = try!(self.check_size(res));
        Ok(&buf[..res])
    }

    pub fn read_timeout<'b>(&self, buf: &'b mut [u8], timeout: i32)
                                -> HidResult<&'b [u8]> {
        let res = unsafe {ffi::hid_read_timeout(self._hid_device,
            buf.as_mut_ptr(), buf.len() as size_t, timeout)};
        let res = try!(self.check_size(res));
        Ok(&buf[..res])
    }

    pub fn send_feature_report(&self, data: &[u8]) -> HidResult<()> {
        let res = unsafe {ffi::hid_send_feature_report(self._hid_device,
            data.as_ptr(), data.len() as size_t)};
        let res = try!(self.check_size(res));
        if res != data.len() {
            Err("Failed to send feature report completely")
        } else {
            Ok(())
        }
    }

    pub fn get_feature_report<'b>(&self, buf: &'b mut [u8], report_id: u8)
                                    -> HidResult<&'b [u8]> {
        buf[0] = report_id;
        let res = unsafe {ffi::hid_get_feature_report(self._hid_device,
            buf.as_mut_ptr(), buf.len() as size_t)};
        let res = try!(self.check_size(res));
        if res == 0 {
            Err("Zero length, at least one byte was expected")
        } else {
            Ok(&buf[1..res])
        }
    }

    pub fn set_blocking_mode(&self, blocking: bool) -> HidResult<()> {
        let res = unsafe {ffi::hid_set_nonblocking(self._hid_device,
            if blocking {0i32} else {1i32} )};
        if res == -1 {
            Err("Failed to set blocking mode")
        } else {
            Ok(())
        }
    }

    pub fn get_manufacturer_string(&self) -> HidResult<String> {
        let mut buf = [0i32; STRING_BUF_LEN];
        let res = unsafe {ffi::hid_get_manufacturer_string(self._hid_device,
            buf.as_mut_ptr(), STRING_BUF_LEN as size_t)};
        let res = try!(self.check_size(res));
        unsafe{wchar_to_string(buf[..res].as_ptr())}
    }

    pub fn get_product_string(&self) -> HidResult<String> {
        let mut buf = [0i32; STRING_BUF_LEN];
        let res = unsafe {ffi::hid_get_product_string(self._hid_device,
            buf.as_mut_ptr(), STRING_BUF_LEN as size_t)};
        let res = try!(self.check_size(res));
        unsafe{wchar_to_string(buf[..res].as_ptr())}
    }

    pub fn get_serial_number_string(&self) -> HidResult<String> {
        let mut buf = [0i32; STRING_BUF_LEN];
        let res = unsafe {ffi::hid_get_serial_number_string(self._hid_device,
            buf.as_mut_ptr(), STRING_BUF_LEN as size_t)};
        let res = try!(self.check_size(res));
        unsafe{wchar_to_string(buf[..res].as_ptr())}
    }

    //TODO implement get_indexed_strings
}