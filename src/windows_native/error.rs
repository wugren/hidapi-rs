use windows_sys::Win32::Devices::DeviceAndDriverInstallation::*;
use crate::HidError;

pub type WinResult<T> = Result<T, WinError>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WinError {
    Config(CONFIGRET),
    BufferTooSmall,
    InvalidDeviceId,
    InvalidDeviceNode,
    NoSuchValue,
    WrongPropertyDataType,
    UnexpectedReturnSize
}

impl From<WinError> for HidError {
    fn from(value: WinError) -> Self {
        HidError::HidApiError { message: format!("WinError: {:?}", value)}
    }
}

fn config_to_error(ret: CONFIGRET) -> WinError {
    match ret {
        CR_BUFFER_SMALL => WinError::BufferTooSmall,
        CR_INVALID_DEVICE_ID => WinError::InvalidDeviceId,
        CR_INVALID_DEVNODE => WinError::InvalidDeviceNode,
        CR_NO_SUCH_VALUE => WinError::NoSuchValue,
        ret => WinError::Config(ret)
    }

}

pub fn check_config(ret: CONFIGRET, expected: CONFIGRET) -> WinResult<()>  {
    if ret == expected {
        Ok(())
    } else {
        Err(config_to_error(ret))
    }
}