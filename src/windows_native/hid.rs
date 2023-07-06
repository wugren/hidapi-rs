use std::mem::{size_of, zeroed};
use windows_sys::core::GUID;
use windows_sys::Win32::Devices::HumanInterfaceDevice::{HIDD_ATTRIBUTES, HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetHidGuid, HidD_GetPreparsedData, HIDP_CAPS, HidP_GetCaps, HIDP_STATUS_SUCCESS};
use crate::ensure;
use crate::windows_native::error::{check_boolean, WinError, WinResult};
use crate::windows_native::types::Handle;

pub fn get_interface_guid() -> GUID {
    unsafe {
        let mut guid = zeroed();
        HidD_GetHidGuid(&mut guid);
        guid
    }
}

pub fn get_hid_attributes(handle: &Handle) -> HIDD_ATTRIBUTES {
    unsafe {
        let mut attrib = HIDD_ATTRIBUTES {
            Size: size_of::<HIDD_ATTRIBUTES>() as u32,
            ..zeroed()
        };
        HidD_GetAttributes(handle.as_raw(), &mut attrib);
        attrib
    }
}

pub fn get_hid_caps(handle: &Handle) -> WinResult<HIDP_CAPS> {
    unsafe {
        let mut caps = zeroed();
        let mut pp_data = 0;
        check_boolean(HidD_GetPreparsedData(handle.as_raw(), &mut pp_data))?;
        let r = HidP_GetCaps(pp_data, &mut caps);
        HidD_FreePreparsedData(pp_data);
        ensure!(r == HIDP_STATUS_SUCCESS, Err(WinError::InvalidPreparsedData));
        Ok(caps)
    }
}