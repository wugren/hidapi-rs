//! This backend uses libudev to discover devices and then talks to hidraw directly

use std::{
    cell::{Cell, Ref, RefCell},
    convert::TryInto,
    ffi::{CStr, CString, OsStr, OsString},
    fs::{File, OpenOptions},
    io::Read,
    os::{
        fd::{AsRawFd, OwnedFd},
        unix::{ffi::OsStringExt, fs::OpenOptionsExt},
    },
    path::{Path, PathBuf},
};

use nix::{
    errno::Errno,
    ioctl_read, ioctl_readwrite_buf,
    poll::{poll, PollFd, PollFlags},
    sys::stat::{fstat, major, minor},
    unistd::{read, write},
};

use super::{BusType, DeviceInfo, HidDeviceBackend, HidError, HidResult, WcharString};

pub struct HidApiBackend;

impl HidApiBackend {
    pub fn get_hid_device_info_vector() -> HidResult<Vec<DeviceInfo>> {
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
            if let Some(mut device) = device_to_hid_device_info(&device) {
                devices.append(&mut device);
            }
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
}

// Bus values from linux/input.h
const BUS_USB: u16 = 0x03;
const BUS_BLUETOOTH: u16 = 0x05;
const BUS_I2C: u16 = 0x18;
const BUS_SPI: u16 = 0x1C;

fn device_to_hid_device_info(raw_device: &udev::Device) -> Option<Vec<DeviceInfo>> {
    let mut infos = Vec::new();

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
    let info = match bus_type {
        BusType::Usb => fill_in_usb(raw_device, info, name),
        _ => DeviceInfo {
            manufacturer_string: WcharString::String("".into()),
            product_string: osstring_to_string(name.into()),
            ..info
        },
    };

    if let Ok(descriptor) = HidrawReportDescriptor::from_syspath(raw_device.syspath()) {
        let mut usages = descriptor.usages();

        // Get the first usage page and usage for our current DeviceInfo
        if let Some((usage_page, usage)) = usages.next() {
            infos.push(DeviceInfo {
                usage_page,
                usage,
                ..info
            });

            // Now we can create DeviceInfo for all the other usages
            for (usage_page, usage) in usages {
                let prev = infos.last().unwrap();

                infos.push(DeviceInfo {
                    usage_page,
                    usage,
                    ..prev.clone()
                })
            }
        }
    } else {
        infos.push(info);
    }

    Some(infos)
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

// From linux/hid.h
const HID_MAX_DESCRIPTOR_SIZE: usize = 4096;

// From linux/hidraw.h
struct HidrawReportDescriptor {
    size: u32,
    value: [u8; HID_MAX_DESCRIPTOR_SIZE],
}

impl Default for HidrawReportDescriptor {
    fn default() -> Self {
        Self {
            size: 0,
            value: [0u8; HID_MAX_DESCRIPTOR_SIZE],
        }
    }
}

impl HidrawReportDescriptor {
    /// Open and parse given the "base" sysfs of the device
    pub fn from_syspath(syspath: &Path) -> HidResult<Self> {
        let mut descriptor = HidrawReportDescriptor::default();

        let path = syspath.join("device/report_descriptor");
        let mut f = File::open(path)?;
        let len = f.read(&mut descriptor.value)?;
        descriptor.size = len as u32;

        Ok(descriptor)
    }

    /// Create a descriptor from a slice
    ///
    /// It returns an error if the value slice is too large for it to be a HID
    /// descriptor
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn from_slice(value: &[u8]) -> HidResult<Self> {
        let size: u32 = match value.len().try_into() {
            Ok(v) => v,
            Err(_) => {
                return Err(HidError::HidApiError {
                    message: "HID report descriptor over 4kB".into(),
                })
            }
        };

        let mut desc = Self {
            size,
            ..Default::default()
        };
        desc.value[..value.len()].copy_from_slice(value);

        Ok(desc)
    }

    pub fn usages(&self) -> impl Iterator<Item = (u16, u16)> + '_ {
        UsageIterator {
            initial: true,
            usage_page: 0,
            value: &self.value,
        }
    }
}

/// Iterates over the values in a HidrawReportDescriptor
struct UsageIterator<'a> {
    initial: bool,
    usage_page: u16,
    value: &'a [u8],
}

impl<'a> Iterator for UsageIterator<'a> {
    type Item = (u16, u16);

    fn next(&mut self) -> Option<Self::Item> {
        let (usage_page, page, advanced) =
            match next_hid_usage(self.value, self.initial, self.usage_page) {
                Some(n) => n,
                None => return None,
            };

        self.usage_page = usage_page;
        self.initial = false;
        self.value = &self.value[advanced..];
        Some((usage_page, page))
    }
}

// This comes from hidapi which apparently comes from Apple's implementation of
// this
fn next_hid_usage(
    mut desc: &[u8],
    initial: bool,
    mut usage_page: u16,
) -> Option<(u16, u16, usize)> {
    let mut usage = None;
    let mut usage_pair = None;
    let mut advanced = 0_usize;

    while !desc.is_empty() {
        let key = desc[0];
        let key_cmd = key & 0xfc;

        let (data_len, key_size) = match hid_item_size(desc) {
            Some(v) => v,
            None => return None,
        };

        match key_cmd {
            // Usage Page 6.2.2.7 (Global)
            0x4 => {
                usage_page = hid_report_bytes(desc, data_len) as u16;
            }
            // Usage 6.2.2.8 (Local)
            0x8 => {
                usage = Some(hid_report_bytes(desc, data_len) as u16);
            }
            // Collection 6.2.2.4 (Main)
            0xa0 => {
                // Usage is a Local Item, unset it
                if let Some(u) = usage.take() {
                    usage_pair = Some((usage_page, u))
                }
            }
            // Input 6.2.2.4 (Main)
		        0x80 |
            // Output 6.2.2.4 (Main)
		        0x90 |
            // Feature 6.2.2.4 (Main)
		        0xb0 |
            // End Collection 6.2.2.4 (Main)
		        0xc0  =>  {
			          // Usage is a Local Item, unset it
                usage.take();
            }
            _ => {}
        }

        advanced += data_len + key_size;
        desc = &desc[(data_len + key_size)..];
        if let Some((usage_page, usage)) = usage_pair {
            return Some((usage_page, usage, advanced));
        }
    }

    if let (true, Some(usage)) = (initial, usage) {
        return Some((usage_page, usage, advanced));
    }

    None
}

/// Gets the size of the HID item at the given position
///
/// Returns data_len and key_size when successful
fn hid_item_size(desc: &[u8]) -> Option<(usize, usize)> {
    let key = desc[0];

    // Long Item. Next byte contains the length of the data section.
    if (key & 0xf0) == 0xf0 {
        if !desc.is_empty() {
            return Some((desc[1].into(), 3));
        }

        // Malformed report
        return None;
    }

    // Short Item. Bottom two bits contains the size code
    match key & 0x03 {
        v @ 0 | v @ 1 | v @ 2 => Some((v.into(), 1)),
        3 => Some((4, 1)),
        _ => unreachable!(), // & 0x03 means this can't happen
    }
}

/// Get the bytes from a HID report descriptor
///
/// Must only be called with `num_bytes` 0, 1, 2 or 4.
fn hid_report_bytes(desc: &[u8], num_bytes: usize) -> u32 {
    if num_bytes >= desc.len() {
        return 0;
    }

    match num_bytes {
        0 => 0,
        1 => desc[1] as u32,
        2 => {
            let bytes = [desc[1], desc[2], 0, 0];
            u32::from_le_bytes(bytes)
        }
        4 => u32::from_le_bytes(desc[1..=4].try_into().unwrap()),
        _ => unreachable!(),
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
const HIDRAW_IOC_GRDESCSIZE: u8 = 0x01;
const HIDRAW_SET_FEATURE: u8 = 0x06;
const HIDRAW_GET_FEATURE: u8 = 0x07;

ioctl_read!(
    hidraw_ioc_grdescsize,
    HIDRAW_IOC_MAGIC,
    HIDRAW_IOC_GRDESCSIZE,
    i32
);

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
    blocking: Cell<bool>,
    fd: OwnedFd,
    info: RefCell<Option<DeviceInfo>>,
}

unsafe impl Send for HidDevice {}

// API for the library to call us, or for internal uses
impl HidDevice {
    pub(crate) fn open(vid: u16, pid: u16, sn: Option<&str>) -> HidResult<Self> {
        for device in HidApiBackend::get_hid_device_info_vector()?
            .iter()
            .filter(|device| device.vendor_id == vid && device.product_id == pid)
        {
            match (sn, &device.serial_number) {
                (None, _) => return Self::open_path(&device.path),
                (Some(sn), WcharString::String(serial_number)) if sn == serial_number => {
                    return Self::open_path(&device.path)
                }
                _ => continue,
            };
        }

        Err(HidError::HidApiError {
            message: "device not found".into(),
        })
    }

    pub(crate) fn open_path(device_path: &CStr) -> HidResult<HidDevice> {
        // Paths on Linux can be anything but devnode paths are going to be ASCII
        let path = device_path.to_str().expect("path must be utf-8");
        let fd: OwnedFd = match OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(path)
        {
            Ok(f) => f.into(),
            Err(e) => {
                return Err(HidError::HidApiError {
                    message: format!("failed to open device with path {path}: {e}"),
                });
            }
        };

        let mut size = 0_i32;
        if let Err(e) = unsafe { hidraw_ioc_grdescsize(fd.as_raw_fd(), &mut size) } {
            return Err(HidError::HidApiError {
                message: format!("ioctl(GRDESCSIZE) error for {path}, not a HIDRAW device?: {e}"),
            });
        }

        Ok(Self {
            blocking: Cell::new(true),
            fd,
            info: RefCell::new(None),
        })
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

impl HidDeviceBackend for HidDevice {
    fn write(&self, data: &[u8]) -> HidResult<usize> {
        if data.is_empty() {
            return Err(HidError::InvalidZeroSizeData);
        }

        Ok(write(self.fd.as_raw_fd(), data)?)
    }

    fn read(&self, buf: &mut [u8]) -> HidResult<usize> {
        // If the caller asked for blocking, -1 makes us wait forever
        let timeout = if self.blocking.get() { -1 } else { 0 };
        self.read_timeout(buf, timeout)
    }

    fn read_timeout(&self, buf: &mut [u8], timeout: i32) -> HidResult<usize> {
        let pollfd = PollFd::new(self.fd.as_raw_fd(), PollFlags::POLLIN);
        let res = poll(&mut [pollfd], timeout)?;

        if res == 0 {
            return Ok(0);
        }

        let events = pollfd
            .revents()
            .map(|e| e.intersects(PollFlags::POLLERR | PollFlags::POLLHUP | PollFlags::POLLNVAL));

        if events.is_none() || events == Some(true) {
            return Err(HidError::HidApiError {
                message: "unexpected poll error (device disconnected)".into(),
            });
        }

        match read(self.fd.as_raw_fd(), buf) {
            Ok(w) => Ok(w),
            Err(Errno::EAGAIN) | Err(Errno::EINPROGRESS) => Ok(0),
            Err(e) => Err(e.into()),
        }
    }

    fn send_feature_report(&self, data: &[u8]) -> HidResult<()> {
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
                return Err(HidError::HidApiError {
                    message: format!("ioctl (GFEATURE): {e}"),
                })
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

    fn get_feature_report(&self, buf: &mut [u8]) -> HidResult<usize> {
        let res = match unsafe { hidraw_ioc_get_feature(self.fd.as_raw_fd(), buf) } {
            Ok(n) => n as usize,
            Err(e) => {
                return Err(HidError::HidApiError {
                    message: format!("ioctl (GFEATURE): {e}"),
                })
            }
        };

        Ok(res)
    }

    fn set_blocking_mode(&self, blocking: bool) -> HidResult<()> {
        self.blocking.set(blocking);
        Ok(())
    }

    fn get_manufacturer_string(&self) -> HidResult<Option<String>> {
        let info = self.info()?;
        Ok(info.manufacturer_string().map(str::to_string))
    }

    fn get_product_string(&self) -> HidResult<Option<String>> {
        let info = self.info()?;
        Ok(info.product_string().map(str::to_string))
    }

    fn get_serial_number_string(&self) -> HidResult<Option<String>> {
        let info = self.info()?;
        Ok(info.serial_number().map(str::to_string))
    }

    fn get_device_info(&self) -> HidResult<DeviceInfo> {
        // What we have is a descriptor to a file in /dev but we need a syspath
        // so we get the major/minor from there and generate our syspath
        let devnum = fstat(self.fd.as_raw_fd())?.st_rdev;
        let syspath: PathBuf = format!("/sys/dev/char/{}:{}", major(devnum), minor(devnum)).into();

        // The clone is a bit silly but we can't implement Copy. Maybe it's not
        // much worse than doing the conversion to Rust from interacting with C.
        let device = udev::Device::from_syspath(&syspath)?;
        match device_to_hid_device_info(&device) {
            Some(info) => Ok(info[0].clone()),
            None => {
                return Err(HidError::HidApiError {
                    message: "failed to create device info".into(),
                })
            }
        }
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

    #[test]
    fn test_hidraw_report_descriptor_1() {
        let data = include_bytes!("../tests/assets/mouse1.data");
        let desc = HidrawReportDescriptor::from_slice(&data[..]).expect("descriptor");
        let values = desc.usages().collect::<Vec<_>>();

        assert_eq!(vec![(65468, 136)], values);
    }

    #[test]
    fn test_hidraw_report_descriptor_2() {
        let data = include_bytes!("../tests/assets/mouse2.data");
        let desc = HidrawReportDescriptor::from_slice(&data[..]).expect("descriptor");
        let values = desc.usages().collect::<Vec<_>>();

        let expected = vec![(1, 2), (1, 1), (1, 128), (12, 1), (65280, 14)];
        assert_eq!(expected, values);
    }
}
