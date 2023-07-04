//! The implementation which uses the C library to perform operations

use std::{
    ffi::CStr,
    fmt::{self, Debug},
};
use std::cell::{Cell, RefCell};
use std::ffi::{c_void, CString};
use std::iter::once;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use bytemuck::{cast_slice, cast_slice_mut};

use windows_sys::core::{GUID, PCWSTR};
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_Device_Interface_List_SizeW, CM_Get_Device_Interface_ListW, CM_Get_Device_Interface_PropertyW, CM_Get_DevNode_PropertyW, CM_Get_Parent, CM_LOCATE_DEVNODE_NORMAL, CM_Locate_DevNodeW, CR_BUFFER_SMALL, CR_SUCCESS};
use windows_sys::Win32::Devices::HumanInterfaceDevice::{HIDD_ATTRIBUTES, HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetHidGuid, HidD_GetManufacturerString, HidD_GetPreparsedData, HidD_GetProductString, HidD_GetSerialNumberString, HidD_SetNumInputBuffers, HIDP_CAPS, HidP_GetCaps, HIDP_STATUS_SUCCESS};
use windows_sys::Win32::Devices::Properties::{DEVPKEY_Device_CompatibleIds, DEVPKEY_Device_ContainerId, DEVPKEY_Device_HardwareIds, DEVPKEY_Device_InstanceId, DEVPKEY_Device_Manufacturer, DEVPKEY_NAME, DEVPROP_TYPE_GUID, DEVPROP_TYPE_STRING, DEVPROP_TYPE_STRING_LIST, DEVPROPKEY, DEVPROPTYPE};
use windows_sys::Win32::Foundation::{BOOLEAN, CloseHandle, ERROR_IO_PENDING, FALSE, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE, TRUE, WAIT_OBJECT_0};
use windows_sys::Win32::Storage::EnhancedStorage::{PKEY_DeviceInterface_Bluetooth_DeviceAddress, PKEY_DeviceInterface_Bluetooth_Manufacturer, PKEY_DeviceInterface_Bluetooth_ModelNumber};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, WriteFile};
use windows_sys::Win32::System::IO::{CancelIo, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
use crate::{BusType, DeviceInfo, HidDeviceBackendBase, HidDeviceBackendWindows, HidError, HidResult, WcharString};

//use crate::{ffi, DeviceInfo, HidDeviceBackendBase, HidError, HidResult, WcharString, HidDeviceBackendWindows, BusType};

//const STRING_BUF_LEN: usize = 128;

macro_rules! ensure {
    ($cond:expr, $result:expr) => {
        if !($cond) {
            return $result;
        }
    };
}


pub struct HidApiBackend;
impl HidApiBackend {
    pub fn get_hid_device_info_vector() -> HidResult<Vec<DeviceInfo>> {
        Ok(enumerate_devices(0, 0))
    }

    pub fn open(vid: u16, pid: u16) -> HidResult<HidDevice> {
        open(vid, pid, None)
    }

    pub fn open_serial(vid: u16, pid: u16, sn: &str) -> HidResult<HidDevice> {
        open(vid, pid, Some(sn))
    }

    pub fn open_path(device_path: &CStr) -> HidResult<HidDevice> {
        open_path(device_path)
    }

}

/// Object for accessing HID device
pub struct HidDevice {
    device_handle: Handle,
    blocking: Cell<bool>,
    output_report_length: u16,
    input_report_length: usize,
    feature_report_length: u16,
    read_pending: bool,
    ol: RefCell<Overlapped>,
    write_ol: RefCell<Overlapped>,
    device_info: DeviceInfo,
    buffer: Cell<Option<Vec<u8>>>,
}

//unsafe impl Send for HidDevice {}

impl Debug for HidDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HidDevice").finish()
    }
}

impl HidDevice {

    fn buffered<R, F>(&self, data: &[u8], min_len: usize, write_func: F) -> R where F: FnOnce(&[u8]) -> R {
        if data.len() >= min_len {
            write_func(data)
        } else {
            let mut write_buf = self
                .buffer
                .take()
                .unwrap_or_else(|| vec![0u8; min_len]);
            write_buf.resize(min_len, 0);
            write_buf[..data.len()].copy_from_slice(data);
            write_buf[data.len()..].fill(0);
            let r = write_func(&write_buf);
            self.buffer.set(Some(write_buf));
            r
        }
    }

}

#[allow(dead_code, unused_variables)]
impl HidDeviceBackendBase for HidDevice {

    fn write(&self, data: &[u8]) -> HidResult<usize> {
        ensure!(!data.is_empty(), Err(HidError::InvalidZeroSizeData));
        let mut written = 0;
        let mut overlapped = self.write_ol.borrow_mut();
        let res = self.buffered(data, self.output_report_length as usize, |data| unsafe {
            WriteFile(self.device_handle.as_raw(), data.as_ptr(), data.len() as u32, null_mut(), overlapped.as_raw())
        });

        if res != TRUE {
            let err = unsafe { GetLastError() };
            ensure!(err == ERROR_IO_PENDING, Err(HidError::IoError { error: std::io::Error::from_raw_os_error(err as _)}));
            let res = unsafe { WaitForSingleObject(overlapped.event_handle(), 1000) };
            assert_eq!(res, WAIT_OBJECT_0);
            let res = unsafe { GetOverlappedResult(self.device_handle.as_raw(), overlapped.as_raw(), &mut written, FALSE) };
            assert_eq!(res, TRUE);
        }

        Ok(written as usize)

    }

    fn read(&self, buf: &mut [u8]) -> HidResult<usize> {
        //let res = unsafe { ffi::hid_read(self._hid_device, buf.as_mut_ptr(), buf.len() as size_t) };
        //self.check_size(res)
        todo!()
    }

    fn read_timeout(&self, buf: &mut [u8], timeout: i32) -> HidResult<usize> {
        //let res = unsafe {
        //    ffi::hid_read_timeout(
        //        self._hid_device,
        //        buf.as_mut_ptr(),
        //        buf.len() as size_t,
        //        timeout,
        //    )
        //};
        //self.check_size(res)
        todo!()
    }

    fn send_feature_report(&self, data: &[u8]) -> HidResult<()> {
        //if data.is_empty() {
        //    return Err(HidError::InvalidZeroSizeData);
        //}
        //let res = unsafe {
        //    ffi::hid_send_feature_report(self._hid_device, data.as_ptr(), data.len() as size_t)
        //};
        //let res = self.check_size(res)?;
        //if res != data.len() {
        //    Err(HidError::IncompleteSendError {
        //        sent: res,
        //        all: data.len(),
        //    })
        //} else {
        //    Ok(())
        //}
        todo!()
    }

    /// Set the first byte of `buf` to the 'Report ID' of the report to be read.
    /// Upon return, the first byte will still contain the Report ID, and the
    /// report data will start in `buf[1]`.
    fn get_feature_report(&self, buf: &mut [u8]) -> HidResult<usize> {
        //let res = unsafe {
        //    ffi::hid_get_feature_report(self._hid_device, buf.as_mut_ptr(), buf.len() as size_t)
        //};
        //self.check_size(res)
        todo!()
    }

    fn set_blocking_mode(&self, blocking: bool) -> HidResult<()> {
        self.blocking.set(blocking);
        Ok(())
        //let res = unsafe {
        //    ffi::hid_set_nonblocking(self._hid_device, if blocking { 0i32 } else { 1i32 })
        //};
        //if res == -1 {
        //    Err(HidError::SetBlockingModeError {
        //        mode: match blocking {
        //            true => "blocking",
        //            false => "not blocking",
        //        },
        //    })
        //} else {
        //    Ok(())
        //}
        //todo!()
    }

    fn get_manufacturer_string(&self) -> HidResult<Option<String>> {
        //let mut buf = [0 as wchar_t; STRING_BUF_LEN];
        //let res = unsafe {
        //    ffi::hid_get_manufacturer_string(
        //        self._hid_device,
        //        buf.as_mut_ptr(),
        //        STRING_BUF_LEN as size_t,
        //    )
        //};
        //let res = self.check_size(res)?;
        //unsafe { Ok(wchar_to_string(buf[..res].as_ptr()).into()) }
        todo!()
    }

    fn get_product_string(&self) -> HidResult<Option<String>> {
        //let mut buf = [0 as wchar_t; STRING_BUF_LEN];
        //let res = unsafe {
        //    ffi::hid_get_product_string(
        //        self._hid_device,
        //        buf.as_mut_ptr(),
        //        STRING_BUF_LEN as size_t,
        //    )
        //};
        //let res = self.check_size(res)?;
        //unsafe { Ok(wchar_to_string(buf[..res].as_ptr()).into()) }

        todo!()
    }

    fn get_serial_number_string(&self) -> HidResult<Option<String>> {
        //let mut buf = [0 as wchar_t; STRING_BUF_LEN];
        //let res = unsafe {
        //    ffi::hid_get_serial_number_string(
        //        self._hid_device,
        //        buf.as_mut_ptr(),
        //        STRING_BUF_LEN as size_t,
        //    )
        //};
        //let res = self.check_size(res)?;
        //unsafe { Ok(wchar_to_string(buf[..res].as_ptr()).into()) }

        todo!()
    }

    fn get_indexed_string(&self, index: i32) -> HidResult<Option<String>> {
        //let mut buf = [0 as wchar_t; STRING_BUF_LEN];
        //let res = unsafe {
        //    ffi::hid_get_indexed_string(
        //        self._hid_device,
        //        index as c_int,
        //        buf.as_mut_ptr(),
        //        STRING_BUF_LEN,
        //    )
        //};
        //let res = self.check_size(res)?;
        //unsafe { Ok(wchar_to_string(buf[..res].as_ptr()).into()) }

        todo!()
    }

    fn get_device_info(&self) -> HidResult<DeviceInfo> {
        Ok(self.device_info.clone())
    }
}

impl HidDeviceBackendWindows for HidDevice {
    fn get_container_id(&self) -> HidResult<GUID> {
        let path = self
            .device_info
            .path
            .to_str()
            .unwrap()
            .encode_utf16()
            .chain(once(0))
            .collect::<Vec<_>>();

        let device_id = get_device_interface_property(path.as_ptr(), &DEVPKEY_Device_InstanceId, DEVPROP_TYPE_STRING)
            .unwrap();

        let dev_node = get_dev_node(cast_slice(&device_id).as_ptr()).unwrap();
        let x = get_devnode_property(dev_node, &DEVPKEY_Device_ContainerId, DEVPROP_TYPE_GUID).unwrap();
        Ok(GUID::from_u128(*bytemuck::from_bytes(&x)))
    }
}

impl Drop for HidDevice {
    fn drop(&mut self) {
        unsafe {
            CancelIo(self.device_handle.as_raw());
        }
    }
}


#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum InternalBuyType {
    Unknown,
    Usb,
    Bluetooth,
    BluetoothLE,
    I2c,
    Spi,
}

impl From<InternalBuyType> for BusType {
    fn from(value: InternalBuyType) -> Self {
        match value {
            InternalBuyType::Unknown => BusType::Unknown,
            InternalBuyType::Usb => BusType::Usb,
            InternalBuyType::Bluetooth => BusType::Bluetooth,
            InternalBuyType::BluetoothLE => BusType::Bluetooth,
            InternalBuyType::I2c => BusType::I2c,
            InternalBuyType::Spi => BusType::Spi
        }
    }
}

struct Handle(HANDLE);

impl Handle {
    fn as_raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
        self.0 = INVALID_HANDLE_VALUE;
    }
}


struct Overlapped(OVERLAPPED);

impl Overlapped {
    fn event_handle(&self) -> HANDLE {
        self.0.hEvent
    }
    fn as_raw(&mut self) -> *mut OVERLAPPED {
        &mut self.0
    }
}

unsafe impl Send for Overlapped { }

impl Default for Overlapped {
    fn default() -> Self {
        Overlapped(unsafe {
            OVERLAPPED {
                //todo check if event is null
                hEvent: CreateEventW(null(), FALSE, FALSE, null()),
                ..zeroed()
            }
        })
    }
}

impl Drop for Overlapped {
    fn drop(&mut self) {
        if self.0.hEvent != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0.hEvent);
            }
        }
        self.0.hEvent = INVALID_HANDLE_VALUE;
    }
}


fn to_upper(u16str: &mut [u16]) {
    for c in u16str {
        if let Ok(t) = u8::try_from(*c) {
            *c = t.to_ascii_uppercase().into();
        }
    }
}

fn find_first_upper_case(u16str: &[u16], pattern: &str) -> Option<usize> {
    u16str
        .windows(pattern.encode_utf16().count())
        .enumerate()
        .filter(|(_, ss)| ss
            .iter()
            .copied()
            .zip(pattern.encode_utf16())
            .all(|(l, r)| l == r))
        .map(|(i, _)| i)
        .next()
}

fn starts_with_ignore_case(utf16str: &[u16], pattern: &str) -> bool {
    //The hidapi c library uses `contains` instead of `starts_with`,
    // but as far as I can tell `starts_with` is a better choice
    char::decode_utf16(utf16str.iter().copied())
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .zip(pattern.chars())
        .all(|(l, r)| l.eq_ignore_ascii_case(&r))
}

fn extract_int_token_value(u16str: &[u16], token: &str) -> Option<u32> {
    let start = find_first_upper_case(u16str, token)? + token.encode_utf16().count();
    char::decode_utf16(u16str[start..].iter().copied())
        .map_while(|c| c
            .ok()
            .and_then(|c| c.to_digit(16)))
        .reduce(|l, r| l * 16 + r)
}

fn u16str_to_wstring(u16str: &[u16]) -> WcharString {
    String::from_utf16(u16str)
        .map(WcharString::String)
        .unwrap_or_else(|_| WcharString::Raw(u16str.to_vec()))
}


fn get_device_interface_property(interface_path: PCWSTR, property_key: &DEVPROPKEY, expected_property_type: DEVPROPTYPE) -> Option<Vec<u8>> {
    let mut property_type = 0;
    let mut len = 0;
    let cr = unsafe {
        CM_Get_Device_Interface_PropertyW(
            interface_path,
            property_key,
            &mut property_type,
            null_mut(),
            &mut len,
            0
        )
    };
    ensure!(cr == CR_BUFFER_SMALL && property_type == expected_property_type, None);
    let mut property_value = vec![0u8; len as usize];
    let cr = unsafe {
        CM_Get_Device_Interface_PropertyW(
            interface_path,
            property_key,
            &mut property_type,
            property_value.as_mut_ptr(),
            &mut len,
            0
        )
    };
    assert_eq!(property_value.len(), len as usize);
    ensure!(cr == CR_SUCCESS, None);
    Some(property_value)
}

fn get_devnode_property(dev_node: u32, property_key: *const DEVPROPKEY, expected_property_type: DEVPROPTYPE) -> Option<Vec<u8>> {
    let mut property_type = 0;
    let mut len = 0;
    let cr = unsafe {
        CM_Get_DevNode_PropertyW(
            dev_node,
            property_key,
            &mut property_type,
            null_mut(),
            &mut len,
            0
        )
    };
    ensure!(cr == CR_BUFFER_SMALL && property_type == expected_property_type, None);
    let mut property_value = vec![0u8; len as usize];
    let cr = unsafe {
        CM_Get_DevNode_PropertyW(
            dev_node,
            property_key,
            &mut property_type,
            property_value.as_mut_ptr(),
            &mut len,
            0
        )
    };
    assert_eq!(property_value.len(), len as usize);
    ensure!(cr == CR_SUCCESS, None);
    Some(property_value)
}

fn get_dev_node_parent(dev_node: u32) -> Option<u32> {
    let mut parent = 0;
    match unsafe { CM_Get_Parent(&mut parent, dev_node, 0)} {
        CR_SUCCESS => Some(parent),
        _ => None
    }
}

fn get_hid_attributes(handle: HANDLE) -> HIDD_ATTRIBUTES {
    unsafe {
        let mut attrib = HIDD_ATTRIBUTES {
            Size: size_of::<HIDD_ATTRIBUTES>() as u32,
            ..zeroed()
        };
        HidD_GetAttributes(handle, &mut attrib);
        attrib
    }
}

fn get_hid_caps(handle: HANDLE) -> Option<HIDP_CAPS> {
    unsafe {
        let mut caps = zeroed();
        let mut pp_data = 0;
        if HidD_GetPreparsedData(handle, &mut pp_data) != 0 {
            let r = HidP_GetCaps(pp_data, &mut caps);
            HidD_FreePreparsedData(pp_data);
            ensure!(r == HIDP_STATUS_SUCCESS, None);
            return Some(caps);
        }
    };
    None
}

fn get_dev_node(path: PCWSTR) -> Option<u32> {
    let mut node = 0;
    let cr = unsafe {
        CM_Locate_DevNodeW(&mut node, path, CM_LOCATE_DEVNODE_NORMAL)
    };
    ensure!(cr == CR_SUCCESS, None);
    Some(node)
}


fn get_interface_list() -> Vec<u16> {
    let interface_class_guid = unsafe {
        let mut guid = std::mem::zeroed();
        HidD_GetHidGuid(&mut guid);
        guid
    };

    let mut device_interface_list = Vec::new();
    loop {
        let mut len = 0;
        let cr = unsafe {
            CM_Get_Device_Interface_List_SizeW(
                &mut len,
                &interface_class_guid,
                null(),
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT)
        };
        assert_eq!(cr, CR_SUCCESS, "Failed to get size of HID device interface list");
        device_interface_list.resize(len as usize, 0);
        let cr = unsafe {
            CM_Get_Device_Interface_ListW(
                &interface_class_guid,
                null(),
                device_interface_list.as_mut_ptr(),
                device_interface_list.len() as u32,
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT
            )
        };
        assert!(cr == CR_SUCCESS || cr == CR_BUFFER_SMALL, "Failed to get HID device interface list");
        if cr == CR_SUCCESS {
            break;
        }
    }
    device_interface_list
}

fn enumerate_devices(vendor_id: u16, product_id: u16) -> Vec<DeviceInfo> {
    get_interface_list()
        .split(|c| *c == 0)
        .filter_map(|device_interface| {
            let device_handle = open_device(device_interface.as_ptr(), false).ok()?;
            let attrib = get_hid_attributes(device_handle.as_raw());
            ((vendor_id == 0 || attrib.VendorID == vendor_id) && (product_id == 0 || attrib.ProductID == product_id))
                .then(|| get_device_info(device_interface, device_handle.as_raw()))
        })
        .collect()
}

fn open_device(path: PCWSTR, open_rw: bool) -> HidResult<Handle> {
    let handle = unsafe {
        CreateFileW(
            path,
            match open_rw {
                true => GENERIC_WRITE | GENERIC_READ,
                false => 0
            },
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            0
        )
    };
    ensure!(handle != INVALID_HANDLE_VALUE, Err(HidError::IoError{ error: std::io::Error::last_os_error() }));
    Ok(Handle(handle))
}

fn read_string(func: unsafe extern "system" fn (HANDLE, *mut c_void, u32) -> BOOLEAN, handle: HANDLE) -> WcharString {
    //Return empty string on failure to match the c implementation
    let mut string = [0u16; 256];
    if unsafe { func(handle, string.as_mut_ptr() as _, (size_of::<u16>() * string.len()) as u32) } != 0 {
        string
            .split(|c| *c == 0)
            .map(u16str_to_wstring)
            .next()
            .unwrap_or_else(|| WcharString::String(String::new()))
    } else {
        //WcharString::None
        WcharString::String(String::new())
    }
}

fn get_device_info(path: &[u16], handle: HANDLE) -> DeviceInfo {
    let attrib = get_hid_attributes(handle);
    let caps = get_hid_caps(handle)
        .unwrap_or(unsafe { zeroed() });

    let mut dev = DeviceInfo {
        path: CString::new(String::from_utf16(path).unwrap()).unwrap(),
        vendor_id: attrib.VendorID,
        product_id: attrib.ProductID,
        serial_number: read_string(HidD_GetSerialNumberString, handle),
        release_number: attrib.VersionNumber,
        manufacturer_string: read_string(HidD_GetManufacturerString, handle),
        product_string: read_string(HidD_GetProductString, handle),
        usage_page: caps.UsagePage,
        usage: caps.Usage,
        interface_number: -1,
        bus_type: BusType::Unknown,
    };

    get_internal_info(path.as_ptr(), &mut dev);
    dev
}

fn get_internal_info(interface_path: PCWSTR, dev: &mut DeviceInfo) -> Option<()> {
    let device_id = get_device_interface_property(interface_path, &DEVPKEY_Device_InstanceId, DEVPROP_TYPE_STRING)?;

    let dev_node = get_dev_node_parent(get_dev_node(cast_slice(&device_id).as_ptr())?)?;

    let compatible_ids = get_devnode_property(dev_node, &DEVPKEY_Device_CompatibleIds, DEVPROP_TYPE_STRING_LIST)?;

    let bus_type = cast_slice(&compatible_ids)
        .split(|c| *c == 0)
        .filter_map(|compatible_id| match compatible_id {
            /* USB devices
		   https://docs.microsoft.com/windows-hardware/drivers/hid/plug-and-play-support
		   https://docs.microsoft.com/windows-hardware/drivers/install/standard-usb-identifiers */
            id if starts_with_ignore_case(id, "USB") => Some(InternalBuyType::Usb),
            /* Bluetooth devices
		   https://docs.microsoft.com/windows-hardware/drivers/bluetooth/installing-a-bluetooth-device */
            id if starts_with_ignore_case(id, "BTHENUM") => Some(InternalBuyType::Bluetooth),
            id if starts_with_ignore_case(id, "BTHLEDEVICE") => Some(InternalBuyType::BluetoothLE),
            /* I2C devices
		   https://docs.microsoft.com/windows-hardware/drivers/hid/plug-and-play-support-and-power-management */
            id if starts_with_ignore_case(id, "PNP0C50") => Some(InternalBuyType::I2c),
            /* SPI devices
		   https://docs.microsoft.com/windows-hardware/drivers/hid/plug-and-play-for-spi */
            id if starts_with_ignore_case(id, "PNP0C51") => Some(InternalBuyType::Spi),
            _ => None
        })
        .next()
        .unwrap_or(InternalBuyType::Unknown);
    dev.bus_type = bus_type.into();
    match bus_type {
        InternalBuyType::Usb => get_usb_info(dev, dev_node),
        InternalBuyType::BluetoothLE => get_ble_info(dev, dev_node),
        _ => None
    };

    Some(())
}

fn get_usb_info(dev: &mut DeviceInfo, mut dev_node: u32) -> Option<()> {
    let mut device_id = get_devnode_property(dev_node, &DEVPKEY_Device_InstanceId, DEVPROP_TYPE_STRING)?;

    to_upper(cast_slice_mut(&mut device_id));
    /* Check for Xbox Common Controller class (XUSB) device.
	   https://docs.microsoft.com/windows/win32/xinput/directinput-and-xusb-devices
	   https://docs.microsoft.com/windows/win32/xinput/xinput-and-directinput
	*/
    if extract_int_token_value(cast_slice(&device_id), "IG_").is_some() {
        dev_node = get_dev_node_parent(dev_node)?;
    }

    let mut hardware_ids = get_devnode_property(dev_node, &DEVPKEY_Device_HardwareIds, DEVPROP_TYPE_STRING_LIST)?;

    /* Get additional information from USB device's Hardware ID
	   https://docs.microsoft.com/windows-hardware/drivers/install/standard-usb-identifiers
	   https://docs.microsoft.com/windows-hardware/drivers/usbcon/enumeration-of-interfaces-not-grouped-in-collections
	*/
    for hardware_id in cast_slice_mut(&mut hardware_ids).split_mut(|c| *c == 0) {
        to_upper(hardware_id);
        if dev.release_number == 0 {
            if let Some(release_number) = extract_int_token_value(hardware_id, "REV_") {
                dev.release_number = release_number as u16;
            }
        }
        if dev.interface_number == -1 {
            if let Some(interface_number) = extract_int_token_value(hardware_id, "MI_") {
                dev.interface_number = interface_number as i32;
            }
        }
    }

    /* Try to get USB device manufacturer string if not provided by HidD_GetManufacturerString. */
    if dev.manufacturer_string().map_or(true, str::is_empty) {
        if let Some(manufacturer_string) = get_devnode_property(dev_node, &DEVPKEY_Device_Manufacturer, DEVPROP_TYPE_STRING) {
            dev.manufacturer_string = u16str_to_wstring(cast_slice(&manufacturer_string));
        }
    }

    /* Try to get USB device serial number if not provided by HidD_GetSerialNumberString. */
    if dev.serial_number().map_or(true, str::is_empty) {
        let mut usb_dev_node = dev_node;
        if dev.interface_number != -1 {
            /* Get devnode parent to reach out composite parent USB device.
               https://docs.microsoft.com/windows-hardware/drivers/usbcon/enumeration-of-the-composite-parent-device
            */
            usb_dev_node = get_dev_node_parent(dev_node)?;
        }

        let device_id = get_devnode_property(usb_dev_node, &DEVPKEY_Device_InstanceId, DEVPROP_TYPE_STRING)?;
        let device_id = cast_slice::<u8, u16>(&device_id);

        /* Extract substring after last '\\' of Instance ID.
		   For USB devices it may contain device's serial number.
		   https://docs.microsoft.com/windows-hardware/drivers/install/instance-ids
		*/
        if let Some(start) = device_id
            .rsplit(|c| *c != b'&' as u16)
            .next()
            .and_then(|s| s.iter().rposition(|c| *c != b'\\' as u16)) {
            dev.serial_number = u16str_to_wstring(&device_id[(start + 1)..]);
        }

    }

    if dev.interface_number == -1 {
        dev.interface_number = 0;
    }

    Some(())
}

/* HidD_GetProductString/HidD_GetManufacturerString/HidD_GetSerialNumberString is not working for BLE HID devices
   Request this info via dev node properties instead.
   https://docs.microsoft.com/answers/questions/401236/hidd-getproductstring-with-ble-hid-device.html
*/
fn get_ble_info(dev: &mut DeviceInfo, dev_node: u32) -> Option<()>{
    if dev.manufacturer_string().map_or(true, str::is_empty) {
        if let Some(manufacturer_string) = get_devnode_property(
            dev_node,
            (&PKEY_DeviceInterface_Bluetooth_Manufacturer as *const PROPERTYKEY) as _,
            DEVPROP_TYPE_STRING) {
            dev.manufacturer_string = u16str_to_wstring(cast_slice(&manufacturer_string));
        }
    }

    if dev.serial_number().map_or(true, str::is_empty) {
        if let Some(serial_number) = get_devnode_property(
            dev_node,
            (&PKEY_DeviceInterface_Bluetooth_DeviceAddress as *const PROPERTYKEY) as _,
            DEVPROP_TYPE_STRING) {
            dev.serial_number = u16str_to_wstring(cast_slice(&serial_number));
        }
    }

    if dev.product_string().map_or(true, str::is_empty) {
        let product_string = get_devnode_property(
            dev_node,
            (&PKEY_DeviceInterface_Bluetooth_ModelNumber as *const PROPERTYKEY) as _,
            DEVPROP_TYPE_STRING
        ).or_else(|| {
            /* Fallback: Get devnode grandparent to reach out Bluetooth LE device node */
            get_dev_node_parent(dev_node)
                .and_then(|parent_dev_node| get_devnode_property(parent_dev_node, &DEVPKEY_NAME, DEVPROP_TYPE_STRING))
        });
        if let Some(product_string) = product_string {
            dev.product_string = u16str_to_wstring(cast_slice(&product_string));
        }
    }

    Some(())
}

fn open(vid: u16, pid: u16, sn: Option<&str>) -> HidResult<HidDevice> {
    let dev = enumerate_devices(vid, pid)
        .into_iter()
        .filter(|dev| dev.vendor_id == vid && dev.product_id == pid)
        .filter(|dev| sn.map_or(true, |sn| dev.serial_number().is_some_and(|n| sn == n)))
        .next()
        .ok_or(HidError::HidApiErrorEmpty)?;
    open_path(dev.path())
}

fn open_path(device_path: &CStr) -> HidResult<HidDevice> {
    let device_path = device_path
        .to_str()
        .map(|s| s
            .encode_utf16()
            .chain(once(0))
            .collect::<Vec<_>>())
        .unwrap();
    let handle = open_device(device_path.as_ptr(), true)?;
    assert_ne!(unsafe { HidD_SetNumInputBuffers(handle.as_raw(), 64) }, 0);
    assert_ne!(unsafe { HidD_SetNumInputBuffers(handle.as_raw(), 64) }, 0);
    let caps = get_hid_caps(handle.as_raw()).unwrap();
    let device_info = get_device_info(&device_path, handle.as_raw());
    let dev = HidDevice {
        device_handle: handle,
        blocking: Cell::new(true),
        output_report_length: caps.OutputReportByteLength,
        input_report_length: caps.InputReportByteLength as usize,
        feature_report_length: caps.FeatureReportByteLength,
        //feature_buf: null_mut(),
        read_pending: false,
        //read_buf: null_mut(),
        ol: Default::default(),
        write_ol: Default::default(),
        device_info,
        buffer: Cell::new(None),
    };

    Ok(dev)
}