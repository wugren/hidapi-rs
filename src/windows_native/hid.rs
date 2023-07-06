use std::mem::{size_of, zeroed};
use windows_sys::core::GUID;
use windows_sys::Win32::Devices::HumanInterfaceDevice::{HIDD_ATTRIBUTES, HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetHidGuid, HidD_GetPreparsedData, HIDP_CAPS, HidP_GetCaps, HIDP_STATUS_SUCCESS};
use windows_sys::Win32::Foundation::HANDLE;
use crate::ensure;

pub fn get_interface_guid() -> GUID {
    unsafe {
        let mut guid = zeroed();
        HidD_GetHidGuid(&mut guid);
        guid
    }
}

pub fn get_hid_attributes(handle: HANDLE) -> HIDD_ATTRIBUTES {
    unsafe {
        let mut attrib = HIDD_ATTRIBUTES {
            Size: size_of::<HIDD_ATTRIBUTES>() as u32,
            ..zeroed()
        };
        HidD_GetAttributes(handle, &mut attrib);
        attrib
    }
}

pub fn get_hid_caps(handle: HANDLE) -> Option<HIDP_CAPS> {
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