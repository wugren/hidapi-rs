use std::ptr::null_mut;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{CM_Get_DevNode_PropertyW, CM_Get_Parent, CM_LOCATE_DEVNODE_NORMAL, CM_Locate_DevNodeW, CR_BUFFER_SMALL};
use windows_sys::Win32::Devices::Properties::{DEVPROPKEY, DEVPROPTYPE};
use crate::ensure;
use crate::windows_native::error::{check_config, WinError, WinResult};
use crate::windows_native::types::U16Str;

#[repr(transparent)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct DevNode(u32);

impl DevNode {

    pub fn from_device_id(device_id: &U16Str) -> WinResult<Self> {
        let mut node = 0;
        let cr = unsafe {
            CM_Locate_DevNodeW(&mut node, device_id.as_ptr(), CM_LOCATE_DEVNODE_NORMAL)
        };
        check_config(cr)?;
        Ok(Self(node))
    }

    pub fn parent(self) -> WinResult<Self> {
        let mut parent = 0;
        let cr = unsafe { CM_Get_Parent(&mut parent, self.0, 0) };
        check_config(cr)?;
        Ok(Self(parent))
    }

    pub fn get_property(self, property_key: *const DEVPROPKEY, expected_property_type: DEVPROPTYPE) -> WinResult<Vec<u8>> {
        let mut property_type = 0;
        let mut len = 0;
        let cr = unsafe {
            CM_Get_DevNode_PropertyW(
                self.0,
                property_key,
                &mut property_type,
                null_mut(),
                &mut len,
                0
            )
        };
        ensure!(cr == CR_BUFFER_SMALL && property_type == expected_property_type, Err(WinError::Config(cr)));
        let mut property_value = vec![0u8; len as usize];
        let cr = unsafe {
            CM_Get_DevNode_PropertyW(
                self.0,
                property_key,
                &mut property_type,
                property_value.as_mut_ptr(),
                &mut len,
                0
            )
        };
        assert_eq!(property_value.len(), len as usize);
        check_config(cr)?;
        Ok(property_value)
    }

}