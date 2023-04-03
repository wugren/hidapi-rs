// **************************************************************************
// Copyright (c) 2015 Osspial All Rights Reserved.
//
// This file is part of hidapi-rs, based on hidapi_rust by Roland Ruckerbauer.
// *************************************************************************

//! This crate provides a rust abstraction over the features of the C library
//! hidapi by [signal11](https://github.com/libusb/hidapi).
//!
//! # Usage
//!
//! This crate is [on crates.io](https://crates.io/crates/hidapi) and can be
//! used by adding `hidapi` to the dependencies in your project's `Cargo.toml`.
//!
//! # Example
//!
//! ```rust,no_run
//! extern crate hidapi;
//!
//! use hidapi::HidApi;
//!
//! fn main() {
//!     println!("Printing all available hid devices:");
//!
//!     match HidApi::new() {
//!         Ok(api) => {
//!             for device in api.device_list() {
//!                 println!("{:04x}:{:04x}", device.vendor_id(), device.product_id());
//!             }
//!         },
//!         Err(e) => {
//!             eprintln!("Error: {}", e);
//!         },
//!     }
//! }
//! ```
//!
//! # Feature flags
//!
//! - `linux-static-libusb`: uses statically linked `libusb` backend on Linux
//! - `linux-static-hidraw`: uses statically linked `hidraw` backend on Linux (default)
//! - `linux-shared-libusb`: uses dynamically linked `libusb` backend on Linux
//! - `linux-shared-hidraw`: uses dynamically linked `hidraw` backend on Linux
//! - `linux-shared-udev`: uses dynamically linked `udev` backend on Linux
//! - `illumos-static-libusb`: uses statically linked `libusb` backend on Illumos (default)
//! - `illumos-shared-libusb`: uses statically linked `hidraw` backend on Illumos
//! - `macos-shared-device`: enables shared access to HID devices on MacOS
//!
//! ## Linux backends
//!
//! On linux the libusb backends do not support [`DeviceInfo::usage()`] and [`DeviceInfo::usage_page()`].
//! The hidraw backend has support for them, but it might be buggy in older kernel versions.
//!
//! ## MacOS Shared device access
//!
//! Since `hidapi` 0.12 it is possible to open MacOS devices with shared access, so that multiple
//! [`HidDevice`] handles can access the same physical device. For backward compatibility this is
//! an opt-in that can be enabled with the `macos-shared-device` feature flag.

extern crate libc;
#[cfg(linuxudev)]
extern crate nix;

#[cfg(target_os = "windows")]
extern crate winapi;

mod error;
mod ffi;

#[cfg(not(linuxudev))]
mod hidapi;
#[cfg(linuxudev)]
#[cfg_attr(docsrs, doc(cfg(linuxudev)))]
mod linux_udev;
#[cfg(target_os = "macos")]
#[cfg_attr(docsrs, doc(cfg(target_os = "macos")))]
mod macos;
#[cfg(target_os = "windows")]
#[cfg_attr(docsrs, doc(cfg(target_os = "windows")))]
mod windows;

use libc::wchar_t;
use std::ffi::CStr;
use std::ffi::CString;
use std::fmt;
use std::fmt::Debug;
use std::sync::Mutex;

pub use error::HidError;

#[cfg(not(linuxudev))]
pub use hidapi::HidDevice;
#[cfg(linuxudev)]
pub use linux_udev::HidDevice;

pub type HidResult<T> = Result<T, HidError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitState {
    NotInit,
    Init { enumerate: bool },
}

static INIT_STATE: Mutex<InitState> = Mutex::new(InitState::NotInit);

fn lazy_init(do_enumerate: bool) -> HidResult<()> {
    let mut init_state = INIT_STATE.lock().unwrap();

    match *init_state {
        InitState::NotInit => {
            #[cfg(libusb)]
            if !do_enumerate {
                // Do not scan for devices in libusb_init()
                // Must be set before calling it.
                // This is needed on Android, where access to USB devices is limited
                unsafe { ffi::libusb_set_option(std::ptr::null_mut(), 2) }
            }

            // Initialize the HID
            #[cfg(not(linuxudev))]
            if unsafe { ffi::hid_init() } == -1 {
                return Err(HidError::InitializationError);
            }

            #[cfg(all(target_os = "macos", feature = "macos-shared-device"))]
            unsafe {
                ffi::macos::hid_darwin_set_open_exclusive(0)
            }

            *init_state = InitState::Init {
                enumerate: do_enumerate,
            }
        }
        InitState::Init { enumerate } => {
            if enumerate != do_enumerate {
                panic!("Trying to initialize hidapi with enumeration={}, but it is already initialized with enumeration={}.", do_enumerate, enumerate)
            }
        }
    }

    Ok(())
}

/// `hidapi` context.
///
/// The `hidapi` C library is lazily initialized when creating the first instance,
/// and never deinitialized. Therefore, it is allowed to create multiple `HidApi`
/// instances.
///
/// Each instance has its own device list cache.
pub struct HidApi {
    device_list: Vec<DeviceInfo>,
}

impl HidApi {
    /// Create a new hidapi context.
    ///
    /// Will also initialize the currently available device list.
    ///
    /// # Panics
    ///
    /// Panics if hidapi is already initialized in "without enumerate" mode
    /// (i.e. if `new_without_enumerate()` has been called before).
    pub fn new() -> HidResult<Self> {
        lazy_init(true)?;

        let device_list = HidApi::get_hid_device_info_vector()?;

        Ok(HidApi { device_list })
    }

    /// Create a new hidapi context, in "do not enumerate" mode.
    ///
    /// This is needed on Android, where access to USB device enumeration is limited.
    ///
    /// # Panics
    ///
    /// Panics if hidapi is already initialized in "do enumerate" mode
    /// (i.e. if `new()` has been called before).
    pub fn new_without_enumerate() -> HidResult<Self> {
        lazy_init(false)?;

        Ok(HidApi {
            device_list: Vec::new(),
        })
    }

    /// Refresh devices list and information about them (to access them use
    /// `device_list()` method)
    pub fn refresh_devices(&mut self) -> HidResult<()> {
        let device_list = HidApi::get_hid_device_info_vector()?;
        self.device_list = device_list;
        Ok(())
    }

    #[cfg(not(linuxudev))]
    fn get_hid_device_info_vector() -> HidResult<Vec<DeviceInfo>> {
        let mut device_vector = Vec::with_capacity(8);

        let enumeration = unsafe { ffi::hid_enumerate(0, 0) };
        {
            let mut current_device = enumeration;

            while !current_device.is_null() {
                device_vector.push(unsafe { hidapi::conv_hid_device_info(current_device)? });
                current_device = unsafe { (*current_device).next };
            }
        }

        if !enumeration.is_null() {
            unsafe { ffi::hid_free_enumeration(enumeration) };
        }

        Ok(device_vector)
    }

    #[cfg(linuxudev)]
    fn get_hid_device_info_vector() -> HidResult<Vec<DeviceInfo>> {
        linux_udev::enumerate_devices()
    }
    /// Returns iterator containing information about attached HID devices.
    pub fn device_list(&self) -> impl Iterator<Item = &DeviceInfo> {
        self.device_list.iter()
    }

    /// Open a HID device using a Vendor ID (VID) and Product ID (PID).
    ///
    /// When multiple devices with the same vid and pid are available, then the
    /// first one found in the internal device list will be used. There are however
    /// no guarantees, which device this will be.
    #[cfg(not(linuxudev))]
    pub fn open(&self, vid: u16, pid: u16) -> HidResult<HidDevice> {
        let device = unsafe { ffi::hid_open(vid, pid, std::ptr::null()) };

        if device.is_null() {
            match self.check_error() {
                Ok(err) => Err(err),
                Err(e) => Err(e),
            }
        } else {
            Ok(HidDevice::from_raw(device))
        }
    }

    #[cfg(linuxudev)]
    pub fn open(&self, vid: u16, pid: u16) -> HidResult<HidDevice> {
        HidDevice::open(vid, pid, None)
    }

    /// Open a HID device using a Vendor ID (VID), Product ID (PID) and
    /// a serial number.
    #[cfg(not(linuxudev))]
    pub fn open_serial(&self, vid: u16, pid: u16, sn: &str) -> HidResult<HidDevice> {
        let mut chars = sn.chars().map(|c| c as wchar_t).collect::<Vec<_>>();
        chars.push(0 as wchar_t);
        let device = unsafe { ffi::hid_open(vid, pid, chars.as_ptr()) };
        if device.is_null() {
            match self.check_error() {
                Ok(err) => Err(err),
                Err(e) => Err(e),
            }
        } else {
            Ok(HidDevice::from_raw(device))
        }
    }

    #[cfg(linuxudev)]
    pub fn open_serial(&self, vid: u16, pid: u16, sn: &str) -> HidResult<HidDevice> {
        HidDevice::open(vid, pid, Some(sn))
    }

    /// The path name be determined by inspecting the device list available with [HidApi::devices()](struct.HidApi.html#method.devices)
    ///
    /// Alternatively a platform-specific path name can be used (eg: /dev/hidraw0 on Linux).
    #[cfg(not(linuxudev))]
    pub fn open_path(&self, device_path: &CStr) -> HidResult<HidDevice> {
        let device = unsafe { ffi::hid_open_path(device_path.as_ptr()) };

        if device.is_null() {
            match self.check_error() {
                Ok(err) => Err(err),
                Err(e) => Err(e),
            }
        } else {
            Ok(HidDevice::from_raw(device))
        }
    }

    #[cfg(linuxudev)]
    pub fn open_path(&self, device_path: &CStr) -> HidResult<HidDevice> {
        HidDevice::open_path(device_path)
    }

    /// Open a HID device using libusb_wrap_sys_device.
    #[cfg(libusb)]
    pub fn wrap_sys_device(&self, sys_dev: isize, interface_num: i32) -> HidResult<HidDevice> {
        let device = unsafe { ffi::hid_libusb_wrap_sys_device(sys_dev, interface_num) };

        if device.is_null() {
            match self.check_error() {
                Ok(err) => Err(err),
                Err(e) => Err(e),
            }
        } else {
            Ok(HidDevice::from_raw(device))
        }
    }

    /// Get the last non-device specific error, which happened in the underlying hidapi C library.
    /// To get the last device specific error, use [`HidDevice::check_error`].
    ///
    /// The `Ok()` variant of the result will contain a [HidError::HidApiError](enum.HidError.html).
    ///
    /// When `Err()` is returned, then acquiring the error string from the hidapi C
    /// library failed. The contained [HidError](enum.HidError.html) is the cause, why no error could
    /// be fetched.
    pub fn check_error(&self) -> HidResult<HidError> {
        Ok(HidError::HidApiError {
            message: unsafe {
                match wchar_to_string(ffi::hid_error(std::ptr::null_mut())) {
                    WcharString::String(s) => s,
                    _ => return Err(HidError::HidApiErrorEmpty),
                }
            },
        })
    }
}

/// Converts a pointer to a `*const wchar_t` to a WcharString.
unsafe fn wchar_to_string(wstr: *const wchar_t) -> WcharString {
    if wstr.is_null() {
        return WcharString::None;
    }

    let mut char_vector: Vec<char> = Vec::with_capacity(8);
    let mut raw_vector: Vec<wchar_t> = Vec::with_capacity(8);
    let mut index: isize = 0;
    let mut invalid_char = false;

    let o = |i| *wstr.offset(i);

    while o(index) != 0 {
        use std::char;

        raw_vector.push(*wstr.offset(index));

        if !invalid_char {
            if let Some(c) = char::from_u32(o(index) as u32) {
                char_vector.push(c);
            } else {
                invalid_char = true;
            }
        }

        index += 1;
    }

    if !invalid_char {
        WcharString::String(char_vector.into_iter().collect())
    } else {
        WcharString::Raw(raw_vector)
    }
}

#[derive(Clone, PartialEq)]
enum WcharString {
    String(String),
    Raw(Vec<wchar_t>),
    None,
}

impl Into<Option<String>> for WcharString {
    fn into(self) -> Option<String> {
        match self {
            WcharString::String(s) => Some(s),
            _ => None,
        }
    }
}

/// The underlying HID bus type.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub enum BusType {
    Unknown = 0x00,
    Usb = 0x01,
    Bluetooth = 0x02,
    I2c = 0x03,
    Spi = 0x04,
}

/// Device information. Use accessors to extract information about Hid devices.
///
/// Note: Methods like `serial_number()` may return None, if the conversion to a
/// String failed internally. You can however access the raw hid representation of the
/// string by calling `serial_number_raw()`
#[derive(Clone)]
pub struct DeviceInfo {
    path: CString,
    vendor_id: u16,
    product_id: u16,
    serial_number: WcharString,
    release_number: u16,
    manufacturer_string: WcharString,
    product_string: WcharString,
    #[allow(dead_code)]
    usage_page: u16,
    #[allow(dead_code)]
    usage: u16,
    interface_number: i32,
    bus_type: BusType,
}

impl DeviceInfo {
    pub fn path(&self) -> &CStr {
        &self.path
    }

    pub fn vendor_id(&self) -> u16 {
        self.vendor_id
    }

    pub fn product_id(&self) -> u16 {
        self.product_id
    }

    /// Try to call `serial_number_raw()`, if None is returned.
    pub fn serial_number(&self) -> Option<&str> {
        match self.serial_number {
            WcharString::String(ref s) => Some(s),
            _ => None,
        }
    }

    pub fn serial_number_raw(&self) -> Option<&[wchar_t]> {
        match self.serial_number {
            WcharString::Raw(ref s) => Some(s),
            _ => None,
        }
    }

    pub fn release_number(&self) -> u16 {
        self.release_number
    }

    /// Try to call `manufacturer_string_raw()`, if None is returned.
    pub fn manufacturer_string(&self) -> Option<&str> {
        match self.manufacturer_string {
            WcharString::String(ref s) => Some(s),
            _ => None,
        }
    }

    pub fn manufacturer_string_raw(&self) -> Option<&[wchar_t]> {
        match self.manufacturer_string {
            WcharString::Raw(ref s) => Some(s),
            _ => None,
        }
    }

    /// Try to call `product_string_raw()`, if None is returned.
    pub fn product_string(&self) -> Option<&str> {
        match self.product_string {
            WcharString::String(ref s) => Some(s),
            _ => None,
        }
    }

    pub fn product_string_raw(&self) -> Option<&[wchar_t]> {
        match self.product_string {
            WcharString::Raw(ref s) => Some(s),
            _ => None,
        }
    }

    /// Usage page is not available on linux libusb backends
    #[cfg(not(all(libusb, target_os = "linux")))]
    pub fn usage_page(&self) -> u16 {
        self.usage_page
    }

    /// Usage is not available on linux libusb backends
    #[cfg(not(all(libusb, target_os = "linux")))]
    pub fn usage(&self) -> u16 {
        self.usage
    }

    pub fn interface_number(&self) -> i32 {
        self.interface_number
    }

    pub fn bus_type(&self) -> BusType {
        self.bus_type
    }

    /// Use the information contained in `DeviceInfo` to open
    /// and return a handle to a [HidDevice](struct.HidDevice.html).
    ///
    /// By default the device path is used to open the device.
    /// When no path is available, then vid, pid and serial number are used.
    /// If both path and serial number are not available, then this function will
    /// fail with [HidError::OpenHidDeviceWithDeviceInfoError](enum.HidError.html#variant.OpenHidDeviceWithDeviceInfoError).
    ///
    /// Note, that opening a device could still be done using [HidApi::open()](struct.HidApi.html#method.open) directly.
    pub fn open_device(&self, hidapi: &HidApi) -> HidResult<HidDevice> {
        if !self.path.as_bytes().is_empty() {
            hidapi.open_path(self.path.as_c_str())
        } else if let Some(sn) = self.serial_number() {
            hidapi.open_serial(self.vendor_id, self.product_id, sn)
        } else {
            Err(HidError::OpenHidDeviceWithDeviceInfoError {
                device_info: Box::new(self.clone()),
            })
        }
    }
}

impl fmt::Debug for DeviceInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HidDeviceInfo")
            .field("vendor_id", &self.vendor_id)
            .field("product_id", &self.product_id)
            .finish()
    }
}
