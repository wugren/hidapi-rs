use std::ptr::{null, null_mut};
use windows_sys::core::PCWSTR;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_Device_Interface_List_SizeW, CM_Get_Device_Interface_ListW, CM_Get_Device_Interface_PropertyW, CR_BUFFER_SMALL, CR_SUCCESS};
use windows_sys::Win32::Devices::Properties::{DEVPROPKEY, DEVPROPTYPE};
use crate::ensure;
use crate::windows_native::hid::get_interface_guid;
use crate::windows_native::types::U16StringList;

pub fn get_device_interface_property(interface_path: PCWSTR, property_key: &DEVPROPKEY, expected_property_type: DEVPROPTYPE) -> Option<Vec<u8>> {
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

pub fn get_interface_list() -> U16StringList {
    let interface_class_guid = get_interface_guid();

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
    U16StringList(device_interface_list)
}
