use std::ptr::addr_of_mut;

use winapi::shared::guiddef::GUID;

use crate::ffi;
use crate::{HidDevice, HidResult};

impl HidDevice {
    /// Get the container ID for a HID device.
    ///
    /// This function returns the `DEVPKEY_Device_ContainerId` property of the
    /// given device. This can be used to correlate different interfaces/ports
    /// on the same hardware device.
    pub fn get_container_id(&self) -> HidResult<GUID> {
        let mut container_id: GUID = unsafe { std::mem::zeroed() };

        let res = unsafe {
            ffi::windows::hid_winapi_get_container_id(self._hid_device, addr_of_mut!(container_id))
        };

        if res == -1 {
            match self.check_error() {
                Ok(err) => Err(err),
                Err(err) => Err(err),
            }
        } else {
            Ok(container_id)
        }
    }
}
