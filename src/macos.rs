use libc::c_int;

use crate::ffi;
use crate::{HidApi, HidDevice, HidResult};

impl HidApi {
    /// **`Only available on MacOS`**
    ///
    /// Changes the behavior of all further calls that open a new [`HidDevice`]
    /// like [`HidApi::open`] or [`HidApi::open_path`]. By default on Darwin
    /// platform all devices opened by [`HidApi`] are opened in exclusive mode.
    ///
    /// When `exclusive` is set to:
    ///   * `false` - all further devices will be opened in non-exclusive mode.
    ///   * `true` all further devices will be opened in exclusive mode.
    pub fn set_open_exclusive(&self, exclusive: bool) {
        unsafe { ffi::macos::hid_darwin_set_open_exclusive(exclusive as c_int) }
    }

    /// **`Only available on MacOS`**
    ///
    /// Get the current opening behavior set by [`HidApi::set_open_exclusive`].
    pub fn get_open_exclusive(&self) -> bool {
        unsafe { ffi::macos::hid_darwin_get_open_exclusive() != 0 }
    }
}

impl HidDevice {
    /// **`Only available on MacOS`**
    ///
    /// Get the location ID for a [`HidDevice`] device.
    pub fn get_location_id(&self) -> HidResult<u32> {
        let mut location_id: u32 = 0;

        let res = unsafe {
            ffi::macos::hid_darwin_get_location_id(self._hid_device, &mut location_id as *mut u32)
        };

        if res == -1 {
            match self.check_error() {
                Ok(err) => Err(err),
                Err(err) => Err(err),
            }
        } else {
            Ok(location_id)
        }
    }

    /// **`Only available on MacOS`**
    ///
    /// Check if the device was opened in exclusive mode.
    pub fn is_open_exclusive(&self) -> HidResult<bool> {
        let res = unsafe { ffi::macos::hid_darwin_is_device_open_exclusive(self._hid_device) };

        if res == -1 {
            match self.check_error() {
                Ok(err) => Err(err),
                Err(err) => Err(err),
            }
        } else {
            Ok(res == 1)
        }
    }
}
