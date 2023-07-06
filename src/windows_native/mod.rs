//! The implementation which uses the C library to perform operations

mod string_utils;
mod types;
mod error;
mod interfaces;
mod hid;
mod device_info;

use std::{
    ffi::CStr,
    fmt::{self, Debug},
};
use std::cell::{Cell, RefCell};
use std::ptr::{null, null_mut};

use windows_sys::core::GUID;
use windows_sys::Win32::Devices::HumanInterfaceDevice::{HidD_GetIndexedString, HidD_SetNumInputBuffers};
use windows_sys::Win32::Devices::Properties::{DEVPKEY_Device_ContainerId, DEVPKEY_Device_InstanceId};
use windows_sys::Win32::Foundation::{ERROR_IO_PENDING, FALSE, GENERIC_READ, GENERIC_WRITE, GetLastError, INVALID_HANDLE_VALUE, TRUE, WAIT_OBJECT_0};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadFile, WriteFile};
use windows_sys::Win32::System::IO::{CancelIo, GetOverlappedResult};
use windows_sys::Win32::System::Threading::{ResetEvent, WaitForSingleObject};
use crate::{DeviceInfo, HidDeviceBackendBase, HidDeviceBackendWindows, HidError, HidResult};
use crate::windows_native::device_info::get_device_info;
use crate::windows_native::error::WinResult;
use crate::windows_native::hid::{get_hid_attributes, get_hid_caps};
use crate::windows_native::interfaces::Interface;
use crate::windows_native::types::{DevNode, Handle, Overlapped, U16Str, U16String};

const STRING_BUF_LEN: usize = 128;

#[macro_export]
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
        Ok(enumerate_devices(0, 0)?)
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
    device_info: DeviceInfo,
    output_report_length: u16,
    input_report_length: usize,
    feature_report_length: u16,
    read_pending: Cell<bool>,
    blocking: Cell<bool>,
    ol: RefCell<Overlapped>,
    write_ol: RefCell<Overlapped>,
    //buffer: Cell<Option<Vec<u8>>>,
}

//unsafe impl Send for HidDevice {}

impl Debug for HidDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HidDevice").finish()
    }
}

//impl HidDevice {
//
//    fn buffered<R, F>(&self, data: &[u8], min_len: usize, write_func: F) -> R where F: FnOnce(&[u8]) -> R {
//        if data.len() >= min_len {
//            write_func(data)
//        } else {
//            let mut write_buf = self
//                .buffer
//                .take()
//                .unwrap_or_else(|| vec![0u8; min_len]);
//            write_buf.resize(min_len, 0);
//            write_buf[..data.len()].copy_from_slice(data);
//            write_buf[data.len()..].fill(0);
//            let r = write_func(&write_buf);
//            self.buffer.set(Some(write_buf));
//            r
//        }
//    }
//
//}

impl HidDeviceBackendBase for HidDevice {

    fn write(&self, data: &[u8]) -> HidResult<usize> {
        ensure!(!data.is_empty(), Err(HidError::InvalidZeroSizeData));
        let mut data = data;
        let mut buf = Vec::new();
        let mut written = 0;
        let mut overlapped = self.write_ol.borrow_mut();

        /* Make sure the right number of bytes are passed to WriteFile. Windows
	   expects the number of bytes which are in the _longest_ report (plus
	   one for the report number) bytes even if the data is a report
	   which is shorter than that. Windows gives us this value in
	   caps.OutputReportByteLength. If a user passes in fewer bytes than this,
	   use cached temporary buffer which is the proper size. */
        if data.len() < self.output_report_length as usize {
            buf.resize( self.output_report_length as usize, 0);
            buf[..data.len()].copy_from_slice(data);
            buf[data.len()..].fill(0);
            data = &buf;
        }

        let res = unsafe {
            WriteFile(self.device_handle.as_raw(), data.as_ptr(), data.len() as u32, null_mut(), overlapped.as_raw())
        };

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
        self.read_timeout(buf, if self.blocking.get() { -1 } else { 0 })
    }

    fn read_timeout(&self, buf: &mut [u8], timeout: i32) -> HidResult<usize> {
        assert!(!buf.is_empty());
        let mut bytes_read = 0;
        let mut overlapped = self.ol.borrow_mut();
        let mut active = false;
        let mut read_buf = vec![0u8; self.input_report_length];

        if !self.read_pending.get() {
            self.read_pending.set(true);

            let res = unsafe {
                ResetEvent(overlapped.event_handle());
                ReadFile(
                    self.device_handle.as_raw(),
                    read_buf.as_mut_ptr() as _,
                    self.input_report_length as u32,
                    &mut bytes_read,
                    overlapped.as_raw())
            };
            if res == FALSE {
                let err = unsafe { GetLastError() };
                if err != ERROR_IO_PENDING {
                    unsafe { CancelIo(self.device_handle.as_raw()) };
                    self.read_pending.set(false);
                    return Err(HidError::HidApiError {message: "dfgdfgdf".to_string() });
                }
                active = true;
            }
        } else {
            active = true;
        }

        if active {
            if timeout >= 0 {
                let res = unsafe { WaitForSingleObject(overlapped.event_handle(), timeout as u32) };
                if res != WAIT_OBJECT_0 {
                    /* There was no data this time. Return zero bytes available,
				        but leave the Overlapped I/O running. */
                    return Ok(0);
                }
            }

            let res = unsafe {
                /* Either WaitForSingleObject() told us that ReadFile has completed, or
		           we are in non-blocking mode. Get the number of bytes read. The actual
		           data has been copied to the data[] array which was passed to ReadFile(). */
                GetOverlappedResult(self.device_handle.as_raw(), overlapped.as_raw(), &mut bytes_read, TRUE)
            };
            if res == FALSE {
                self.read_pending.set(false);
                return Err(HidError::HidApiError { message: "fdgdfg".to_string()});
            }
        }
        self.read_pending.set(false);

        let mut copy_len = 0;
        if bytes_read > 0 {
            /* If report numbers aren't being used, but Windows sticks a report
			   number (0x0) on the beginning of the report anyway. To make this
			   work like the other platforms, and to make it work more like the
			   HID spec, we'll skip over this byte. */
            if read_buf[0] == 0x0 {
                bytes_read -= 1;
                copy_len = usize::min(bytes_read as usize, buf.len());
                buf[..copy_len].copy_from_slice(&read_buf[1..(1 + copy_len)]);
            } else {
                copy_len = usize::min(bytes_read as usize, buf.len());
                buf[..copy_len].copy_from_slice(&read_buf[0..copy_len]);
            }
        }
        Ok(copy_len)
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
    }

    fn get_manufacturer_string(&self) -> HidResult<Option<String>> {
        Ok(self.device_info.manufacturer_string().map(String::from))
    }

    fn get_product_string(&self) -> HidResult<Option<String>> {
        Ok(self.device_info.product_string().map(String::from))
    }

    fn get_serial_number_string(&self) -> HidResult<Option<String>> {
        Ok(self.device_info.serial_number().map(String::from))
    }

    fn get_indexed_string(&self, index: i32) -> HidResult<Option<String>> {
        let mut buf = [0u16; STRING_BUF_LEN];
        let res = unsafe { HidD_GetIndexedString(self.device_handle.as_raw(), index as u32, buf.as_mut_ptr() as _, STRING_BUF_LEN as u32) };
        assert_ne!(res, 0);
        Ok(buf.split(|c| *c == 0).map(String::from_utf16_lossy).next())
    }

    fn get_device_info(&self) -> HidResult<DeviceInfo> {
        Ok(self.device_info.clone())
    }
}

impl HidDeviceBackendWindows for HidDevice {
    fn get_container_id(&self) -> HidResult<GUID> {
        let path = U16String::try_from(self.device_info.path())
            .expect("device path is not valid unicode");

        let device_id: U16String = Interface::get_property(&path, &DEVPKEY_Device_InstanceId)?;

        let dev_node = DevNode::from_device_id(&device_id)?;
        let guid = dev_node.get_property(&DEVPKEY_Device_ContainerId)?;
        Ok(guid)
    }
}

impl Drop for HidDevice {
    fn drop(&mut self) {
        unsafe {
            CancelIo(self.device_handle.as_raw());
        }
    }
}


fn enumerate_devices(vendor_id: u16, product_id: u16) -> WinResult<Vec<DeviceInfo>> {
    Ok(Interface::get_interface_list()?
        .iter()
        .filter_map(|device_interface| {
            let device_handle = open_device(device_interface, false).ok()?;
            let attrib = get_hid_attributes(&device_handle);
            ((vendor_id == 0 || attrib.VendorID == vendor_id) && (product_id == 0 || attrib.ProductID == product_id))
                .then(|| get_device_info(device_interface, &device_handle))
        })
        .collect())
}

fn open_device(path: &U16Str, open_rw: bool) -> HidResult<Handle> {
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
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
    Ok(Handle::from_raw(handle))
}

fn open(vid: u16, pid: u16, sn: Option<&str>) -> HidResult<HidDevice> {
    let dev = enumerate_devices(vid, pid)?
        .into_iter()
        .filter(|dev| dev.vendor_id == vid && dev.product_id == pid)
        .find(|dev| sn.map_or(true, |sn| dev.serial_number().is_some_and(|n| sn == n)))
        .ok_or(HidError::HidApiErrorEmpty)?;
    open_path(dev.path())
}

fn open_path(device_path: &CStr) -> HidResult<HidDevice> {
    let device_path = U16String::try_from(device_path)
        .unwrap();
    let handle = open_device(&device_path, true)?;
    assert_ne!(unsafe { HidD_SetNumInputBuffers(handle.as_raw(), 64) }, 0);
    let caps = get_hid_caps(&handle)?;
    let device_info = get_device_info(&device_path, &handle);
    let dev = HidDevice {
        device_handle: handle,
        blocking: Cell::new(true),
        output_report_length: caps.OutputReportByteLength,
        input_report_length: caps.InputReportByteLength as usize,
        feature_report_length: caps.FeatureReportByteLength,
        //feature_buf: null_mut(),
        read_pending: Cell::new(false),
        //read_buf: null_mut(),
        ol: Default::default(),
        write_ol: Default::default(),
        device_info,
        //buffer: Cell::new(None),
    };

    Ok(dev)
}