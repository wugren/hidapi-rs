use libc::{c_void, c_char, c_ushort, wchar_t, c_int, c_uchar, size_t};

pub type HidDevice = c_void;

pub struct HidDeviceInfo {
    path: *const c_char,
    vendor_id: c_ushort,
    product_id: c_ushort,
    serial_number: *const wchar_t,
    release_number: c_ushort,
    manufactor_string: *const wchar_t,
    product_string: *const wchar_t,
    usage_page: c_ushort,
    usage: c_ushort,
    interface_number: c_int,
    next: *const HidDeviceInfo,
}

#[cfg(target_os = "windows")]
#[link(name = "hidapi")]
extern "C" {
    pub fn hid_init() -> c_int;
    pub fn hid_exit() -> c_int;
    pub fn hid_enumerate(vendor_id: c_ushort, product_id: c_ushort) -> *const HidDeviceInfo;
    pub fn hid_free_enumeration(hid_device_info: *const HidDeviceInfo);
    pub fn hid_open(vendor_id: c_ushort, product_id: c_ushort, serial_number: *const wchar_t)
            -> *const HidDevice;
    pub fn hid_open_path(path: *const c_char) -> *const HidDevice;
    pub fn hid_write(device: *const HidDevice, data: *const c_uchar, length: size_t) -> c_int;
    pub fn hid_read_timeout(device: *const HidDevice, data: *const c_uchar, length: size_t,
            milleseconds: c_int) -> c_int;
    pub fn hid_read(device: *const HidDevice, data: *const c_uchar, length: size_t) -> c_int;
    pub fn hid_set_nonblocking(device: *const HidDevice, nonblock: c_int) -> c_int;
    pub fn hid_send_feature_report(device: *const HidDevice, data: *const c_uchar, length: size_t)
            -> c_int;
    pub fn hid_get_feature_report(device: *const HidDevice, data: *const c_uchar, length: size_t)
            -> c_int;
    pub fn hid_close(device: *const HidDevice);
    pub fn hid_get_manufacturer_string(device: *const HidDevice, string: *const wchar_t,
            maxlen: size_t) -> c_int;
    pub fn hid_get_product_string(device: *const HidDevice, string: *const wchar_t, maxlen: size_t)
            -> c_int;
    pub fn hid_get_serial_number_string(device: *const HidDevice, string: *const wchar_t,
            maxlen: size_t) -> c_int;
    pub fn hid_get_indexed_string(device: *const HidDevice, string_index: c_int,
            string: *const wchar_t, maxlen: size_t) -> c_int;
    pub fn hid_error(device: *const HidDevice) -> *const wchar_t;
}

#[cfg(target_os = "linux")]
#[link(name = "hidapi-hidraw")]
extern "C" {
    pub fn hid_init() -> c_int;
    pub fn hid_exit() -> c_int;
    pub fn hid_enumerate(vendor_id: c_ushort, product_id: c_ushort) -> *const HidDeviceInfo;
    pub fn hid_free_enumeration(hid_device_info: *const HidDeviceInfo);
    pub fn hid_open(vendor_id: c_ushort, product_id: c_ushort, serial_number: *const wchar_t)
            -> *const HidDevice;
    pub fn hid_open_path(path: *const c_char) -> *const HidDevice;
    pub fn hid_write(device: *const HidDevice, data: *const c_uchar, length: size_t) -> c_int;
    pub fn hid_read_timeout(device: *const HidDevice, data: *const c_uchar, length: size_t,
            milleseconds: c_int) -> c_int;
    pub fn hid_read(device: *const HidDevice, data: *const c_uchar, length: size_t) -> c_int;
    pub fn hid_set_nonblocking(device: *const HidDevice, nonblock: c_int) -> c_int;
    pub fn hid_send_feature_report(device: *const HidDevice, data: *const c_uchar, length: size_t)
            -> c_int;
    pub fn hid_get_feature_report(device: *const HidDevice, data: *const c_uchar, length: size_t)
            -> c_int;
    pub fn hid_close(device: *const HidDevice);
    pub fn hid_get_manufacturer_string(device: *const HidDevice, string: *const wchar_t,
            maxlen: size_t) -> c_int;
    pub fn hid_get_product_string(device: *const HidDevice, string: *const wchar_t, maxlen: size_t)
            -> c_int;
    pub fn hid_get_serial_number_string(device: *const HidDevice, string: *const wchar_t,
            maxlen: size_t) -> c_int;
    pub fn hid_get_indexed_string(device: *const HidDevice, string_index: c_int,
            string: *const wchar_t, maxlen: size_t) -> c_int;
    pub fn hid_error(device: *const HidDevice) -> *const wchar_t;
}
