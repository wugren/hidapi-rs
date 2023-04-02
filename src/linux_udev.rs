//! Functions for talking to udev directly instead of going through hidapi

extern crate udev;

use std::{
    ffi::{CStr, CString, OsStr, OsString},
    os::unix::ffi::OsStringExt,
};

use super::{BusType, DeviceInfo, HidError, HidResult, WcharString};

/// Enumerate the hidraw devices
pub fn enumerate_devices() -> HidResult<Vec<DeviceInfo>> {
    // This matches what we do with ffi:hid_enumerate but it's not great
    let mut enumerator = match udev::Enumerator::new() {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };
    enumerator.match_subsystem("hidraw").unwrap();
    let scan = match enumerator.scan_devices() {
        Ok(s) => s,
        Err(_) => return Ok(Vec::new()),
    };

    let mut devices = Vec::new();
    for device in scan {
        if let Some(device) = device_to_hid_device_info(&device) {
            devices.push(device);
        }
    }

    Ok(devices)
}

// Bus values from linux/input.h
const BUS_USB: u16 = 0x03;
const BUS_BLUETOOTH: u16 = 0x05;
const BUS_I2C: u16 = 0x18;
const BUS_SPI: u16 = 0x1C;

fn device_to_hid_device_info(raw_device: &udev::Device) -> Option<DeviceInfo> {
    // We're given the hidraw device, but we actually want to go and check out
    // the info for the parent hid device.
    let device = match raw_device.parent_with_subsystem("hid") {
        Ok(Some(dev)) => dev,
        _ => return None,
    };

    let (bus, vid, pid) = match device
        .property_value("HID_ID")
        .and_then(|s| s.to_str())
        .and_then(parse_hid_vid_pid)
    {
        Some(t) => t,
        None => return None,
    };
    let bus_type = match bus {
        BUS_USB => BusType::Usb,
        BUS_BLUETOOTH => BusType::Bluetooth,
        BUS_I2C => BusType::I2c,
        BUS_SPI => BusType::Spi,
        _ => return None,
    };
    let name = match device.property_value("HID_NAME") {
        Some(name) => name,
        None => return None,
    };
    let serial = match device.property_value("HID_UNIQ") {
        Some(serial) => serial,
        None => return None,
    };
    let path = match raw_device
        .devnode()
        .map(|p| p.as_os_str().to_os_string().into_vec())
        .map(CString::new)
    {
        Some(Ok(s)) => s,
        None | Some(Err(_)) => return None,
    };

    // Thus far we've gathered all the common attributes.
    let info = DeviceInfo {
        path,
        vendor_id: vid,
        product_id: pid,
        serial_number: osstring_to_string(serial.into()),
        release_number: 0,
        manufacturer_string: WcharString::None,
        product_string: WcharString::None,
        usage_page: 0,
        usage: 0,
        interface_number: -1,
        bus_type,
    };

    // USB has a bunch more information but everything else gets the same empty
    // manufacturer and the product we read from the property above.
    match bus_type {
        BusType::Usb => Some(fill_in_usb(raw_device, info, name)),
        _ => Some(DeviceInfo {
            manufacturer_string: WcharString::String("".into()),
            product_string: osstring_to_string(name.into()),
            ..info
        }),
    }
}

/// Fill in the extra information that's available for a USB device.
fn fill_in_usb(device: &udev::Device, info: DeviceInfo, name: &OsStr) -> DeviceInfo {
    let usb_dev = match device.parent_with_subsystem_devtype("usb", "usb_device") {
        Ok(Some(dev)) => dev,
        Ok(None) | Err(_) => {
            return DeviceInfo {
                manufacturer_string: WcharString::String("".into()),
                product_string: osstring_to_string(name.into()),
                ..info
            }
        }
    };
    let manufacturer_string = attribute_as_wchar(&usb_dev, "manufacturer");
    let product_string = attribute_as_wchar(&usb_dev, "product");
    let release_number = attribute_as_u16(&usb_dev, "bcdDevice");
    let interface_number = device
        .parent_with_subsystem_devtype("usb", "usb_interface")
        .ok()
        .flatten()
        .map(|ref dev| attribute_as_i32(dev, "bInterfaceNumber"))
        .unwrap_or(-1);

    DeviceInfo {
        release_number,
        manufacturer_string,
        product_string,
        interface_number,
        ..info
    }
}

/// Get the attribute from the device and convert it into a [`WcharString`].
fn attribute_as_wchar(dev: &udev::Device, attr: &str) -> WcharString {
    dev.attribute_value(attr)
        .map(Into::into)
        .map(osstring_to_string)
        .unwrap_or(WcharString::None)
}

/// Get the attribute from the device and convert it into a i32
///
/// On error or if the attribute is not found, it returns -1;
fn attribute_as_i32(dev: &udev::Device, attr: &str) -> i32 {
    dev.attribute_value(attr)
        .and_then(OsStr::to_str)
        .and_then(|v| i32::from_str_radix(v, 16).ok())
        .unwrap_or(-1)
}

/// Get the attribute from the device and convert it into a u16
///
/// On error or if the attribute is not found, it returns 0;
fn attribute_as_u16(dev: &udev::Device, attr: &str) -> u16 {
    dev.attribute_value(attr)
        .and_then(OsStr::to_str)
        .and_then(|v| u16::from_str_radix(v, 16).ok())
        .unwrap_or(0)
}

/// Convert a [`OsString`] into a [`WcharString`]
fn osstring_to_string(s: OsString) -> WcharString {
    match s.into_string() {
        Ok(s) => WcharString::String(s),
        Err(_) => panic!("udev strings should always be utf8"),
    }
}

/// Parse a HID_ID string to find the bus type, the vendor and product id
///
/// These strings would be of the format
///     type vendor   product
///     0003:000005AC:00008242
fn parse_hid_vid_pid(s: &str) -> Option<(u16, u16, u16)> {
    let elems: Vec<Result<u16, _>> = s.split(':').map(|s| u16::from_str_radix(s, 16)).collect();
    if elems.len() != 3 || !elems.iter().all(Result::is_ok) {
        return None;
    };

    let numbers: Vec<u16> = elems.into_iter().map(|n| n.unwrap()).collect();
    Some((numbers[0], numbers[1], numbers[2]))
}

/// Object for accessing the HID device
pub struct HidDevice {
    device: udev::Device,
}

unsafe impl Send for HidDevice {}

// API for the library to call us
impl HidDevice {
    pub(crate) fn open(vid: u16, pid: u16, sn: Option<&str>) -> HidResult<Self> {
        todo!()
    }

    pub(crate) fn open_path(device_path: &CStr) -> HidResult<HidDevice> {
        todo!()
    }
}

// Public API for users
impl HidDevice {
    pub fn check_error(&self) -> HidResult<HidError> {
        todo!()
    }

    pub fn write(&self, data: &[u8]) -> HidResult<usize> {
        todo!()
    }

    pub fn read(&self, buf: &mut [u8]) -> HidResult<usize> {
        todo!()
    }

    pub fn read_timeout(&self, buf: &mut [u8], timeout: i32) -> HidResult<usize> {
        todo!()
    }

    pub fn send_feature_report(&self, data: &[u8]) -> HidResult<()> {
        todo!()
    }

    pub fn get_feature_report(&self, buf: &mut [u8]) -> HidResult<usize> {
        todo!()
    }

    pub fn set_blocking_mode(&self, blocking: bool) -> HidResult<()> {
        todo!()
    }

    pub fn get_manufacturer_string(&self) -> HidResult<Option<String>> {
        todo!()
    }

    pub fn get_product_string(&self) -> HidResult<Option<String>> {
        todo!()
    }

    pub fn get_serial_number_string(&self) -> HidResult<Option<String>> {
        todo!()
    }

    pub fn get_indexed_string(&self, index: i32) -> HidResult<Option<String>> {
        todo!()
    }

    pub fn get_device_info(&self) -> HidResult<DeviceInfo> {
        todo!()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_hid_vid_pid() {
        assert_eq!(None, parse_hid_vid_pid("Hello World"));
        assert_eq!(Some((1, 1, 1)), parse_hid_vid_pid("1:1:1"));
        assert_eq!(Some((0x11, 0x17, 0x18)), parse_hid_vid_pid("11:0017:00018"));
    }
}
