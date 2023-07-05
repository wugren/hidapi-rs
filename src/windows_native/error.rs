use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{CONFIGRET, CR_SUCCESS};

pub type WinResult<T> = Result<T, WinError>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WinError {
    Config(CONFIGRET)
}

pub fn check_config(ret: CONFIGRET) -> WinResult<()>  {
    match ret {
        CR_SUCCESS => Ok(()),
        err => Err(WinError::Config(err))
    }

}