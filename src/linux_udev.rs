//! Functions for talking to udev directly instead of going through hidapi

extern crate udev;

use std::{
    cell::{Cell, Ref, RefCell},
    ffi::{CStr, CString, OsStr, OsString},
    fs::OpenOptions,
    os::{
        fd::{AsRawFd, OwnedFd},
        unix::{ffi::OsStringExt, fs::OpenOptionsExt},
    },
    path::PathBuf,
    sync::Mutex,
};

use nix::{
    errno::Errno,
    ioctl_readwrite_buf,
    poll::{poll, PollFd, PollFlags},
    unistd::{read, write},
};

use super::{BusType, DeviceInfo, HidError, HidResult, WcharString};

/// Global error to simulate what C hidapi does
static GLOBAL_ERROR: Mutex<RefCell<Option<String>>> = Mutex::new(RefCell::new(None));

/// Clear the global error
fn clear_global_error() {
    GLOBAL_ERROR.lock().expect("global error lock").take();
}

/// Register the global error
///
/// It returns an error with his string
fn register_global_error<T>(error: String) -> HidResult<T> {
    GLOBAL_ERROR
        .lock()
        .expect("global error lock")
        .replace(Some(error));

    Err(HidError::HidApiError {
        message: "device not found".into(),
    })
}

pub struct HidApiBackend;

impl HidApiBackend {
    pub fn get_hid_device_info_vector() -> HidResult<Vec<DeviceInfo>> {
        clear_global_error();

        // The C version assumes these can't fail, and they should only fail in case
        // of memory allocation issues, at which point maybe we should panic
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

        // What happens with the C hidapi is that it registers the error, returns
        // NULL, but in this library we still end up returning Ok with an empty
        // vector. So we do the same here.
        #[allow(unused_must_use)]
        if devices.is_empty() {
            register_global_error::<()>("No HID devices found in the system".to_string());
        }

        Ok(devices)
    }

    pub fn open(vid: u16, pid: u16) -> HidResult<HidDevice> {
        HidDevice::open(vid, pid, None)
    }

    pub fn open_serial(vid: u16, pid: u16, sn: &str) -> HidResult<HidDevice> {
        HidDevice::open(vid, pid, Some(sn))
    }

    pub fn open_path(device_path: &CStr) -> HidResult<HidDevice> {
        HidDevice::open_path(device_path)
    }

    pub fn check_error() -> HidResult<HidError> {
        let error = GLOBAL_ERROR.lock().expect("global error lock");
        let borrowed = error.borrow();
        let msg = match borrowed.as_ref() {
            Some(s) => s,
            None => "Success",
        };

        Err(HidError::HidApiError {
            message: msg.to_string(),
        })
    }
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

// From linux/hidraw.h
const HIDRAW_IOC_MAGIC: u8 = b'H';
const HIDRAW_SET_FEATURE: u8 = 0x06;
const HIDRAW_GET_FEATURE: u8 = 0x07;

ioctl_readwrite_buf!(
    hidraw_ioc_set_feature,
    HIDRAW_IOC_MAGIC,
    HIDRAW_SET_FEATURE,
    u8
);
ioctl_readwrite_buf!(
    hidraw_ioc_get_feature,
    HIDRAW_IOC_MAGIC,
    HIDRAW_GET_FEATURE,
    u8
);

/// Object for accessing the HID device
pub struct HidDevice {
    path: PathBuf,
    blocking: Cell<bool>,
    fd: OwnedFd,
    info: RefCell<Option<DeviceInfo>>,
    err: RefCell<Option<String>>,
}

unsafe impl Send for HidDevice {}

// API for the library to call us, or for internal uses
impl HidDevice {
    pub(crate) fn open(vid: u16, pid: u16, sn: Option<&str>) -> HidResult<Self> {
        // TODO: fix this up so we don't copy the serial number
        let sn = sn.map(|s| WcharString::String(s.to_string()));
        for device in HidApiBackend::get_hid_device_info_vector()? {
            if device.vendor_id == vid && device.product_id == pid {
                if sn.is_none() || sn == Some(device.serial_number) {
                    return Self::open_path(&device.path);
                }
            }
        }

        register_global_error("device not found".into())
    }

    pub(crate) fn open_path(device_path: &CStr) -> HidResult<HidDevice> {
        clear_global_error();

        // Paths on Linux can be anything but devnode paths are going to be ASCII
        let path = device_path.to_str().expect("path must be utf-8");
        let file = match OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(path)
        {
            Ok(f) => f,
            Err(e) => return register_global_error(e.to_string()),
        };

        // TODO: maybe add that ioctl check that the C version has
        Ok(Self {
            path: path.into(),
            blocking: Cell::new(true),
            fd: file.into(),
            info: RefCell::new(None),
            err: RefCell::new(None),
        })
    }

    /// Remove the error string for this device
    fn clear_error(&self) {
        self.err.take();
    }

    /// Set the error string for the device.
    ///
    /// For convenience it returns the error.
    fn register_error<T>(&self, error: String) -> HidResult<T> {
        self.err.replace(Some(error.clone()));
        Err(HidError::HidApiError { message: error })
    }
}

// Public API for users
impl HidDevice {
    pub fn check_error(&self) -> HidResult<HidError> {
        let borrow = self.err.borrow();
        let msg = match borrow.as_ref() {
            Some(s) => s,
            None => "Success",
        };

        Ok(HidError::HidApiError {
            message: msg.to_string(),
        })
    }

    pub fn write(&self, data: &[u8]) -> HidResult<usize> {
        if data.is_empty() {
            return Err(HidError::InvalidZeroSizeData);
        }

        match write(self.fd.as_raw_fd(), data) {
            Ok(w) => Ok(w),
            Err(e) => self.register_error(e.to_string()),
        }
    }

    pub fn read(&self, buf: &mut [u8]) -> HidResult<usize> {
        // If the caller asked for blocking, -1 makes us wait forever
        let timeout = if self.blocking.get() { -1 } else { 0 };
        self.read_timeout(buf, timeout)
    }

    pub fn read_timeout(&self, buf: &mut [u8], timeout: i32) -> HidResult<usize> {
        self.clear_error();

        let pollfd = PollFd::new(self.fd.as_raw_fd(), PollFlags::POLLIN);
        let res = poll(&mut [pollfd], timeout)?;

        if res == 0 {
            return Ok(0);
        }

        let events = pollfd
            .revents()
            .map(|e| e.intersects(PollFlags::POLLERR | PollFlags::POLLHUP | PollFlags::POLLNVAL));

        if events.is_none() || events == Some(true) {
            return self.register_error("unexpected poll error (device disconnected)".into());
        }

        match read(self.fd.as_raw_fd(), buf) {
            Ok(w) => Ok(w),
            Err(Errno::EAGAIN) | Err(Errno::EINPROGRESS) => Ok(0),
            Err(e) => self.register_error(e.to_string()),
        }
    }

    pub fn send_feature_report(&self, data: &[u8]) -> HidResult<()> {
        self.clear_error();

        if data.is_empty() {
            return Err(HidError::InvalidZeroSizeData);
        }

        // The ioctl is marked as read-write so we need to mess with the
        // mutability even though nothing should get written
        let res = match unsafe {
            hidraw_ioc_set_feature(self.fd.as_raw_fd(), &mut *(data as *const _ as *mut _))
        } {
            Ok(n) => n as usize,
            Err(e) => {
                let s = format!("ioctl (GFEATURE): {e}");
                return self.register_error(s);
            }
        };

        if res != data.len() {
            return Err(HidError::IncompleteSendError {
                sent: res,
                all: data.len(),
            });
        }

        Ok(())
    }

    pub fn get_feature_report(&self, buf: &mut [u8]) -> HidResult<usize> {
        self.clear_error();

        let res = match unsafe { hidraw_ioc_get_feature(self.fd.as_raw_fd(), buf) } {
            Ok(n) => n as usize,
            Err(e) => {
                let s = format!("ioctl (GFEATURE): {e}");
                return self.register_error(s);
            }
        };

        Ok(res)
    }

    pub fn set_blocking_mode(&self, blocking: bool) -> HidResult<()> {
        self.blocking.set(blocking);
        Ok(())
    }

    pub fn get_manufacturer_string(&self) -> HidResult<Option<String>> {
        let info = self.info()?;
        Ok(info.manufacturer_string().map(str::to_string))
    }

    pub fn get_product_string(&self) -> HidResult<Option<String>> {
        let info = self.info()?;
        Ok(info.product_string().map(str::to_string))
    }

    pub fn get_serial_number_string(&self) -> HidResult<Option<String>> {
        let info = self.info()?;
        Ok(info.serial_number().map(str::to_string))
    }

    pub fn get_indexed_string(&self, _index: i32) -> HidResult<Option<String>> {
        Err(HidError::HidApiError {
            message: "get_indexed_string: not supported".into(),
        })
    }

    pub fn get_device_info(&self) -> HidResult<DeviceInfo> {
        self.clear_error();

        let device = udev::Device::from_syspath(&self.path)?;
        match device_to_hid_device_info(&device) {
            Some(info) => Ok(info),
            None => self.register_error("failed to create device info".into()),
        }
    }

    fn info(&self) -> HidResult<Ref<DeviceInfo>> {
        if self.info.borrow().is_none() {
            let info = self.get_device_info()?;
            self.info.replace(Some(info));
        }

        let info = self.info.borrow();
        Ok(Ref::map(info, |i: &Option<DeviceInfo>| i.as_ref().unwrap()))
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
