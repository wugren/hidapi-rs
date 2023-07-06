use windows_sys::Win32::Devices::DeviceAndDriverInstallation::*;
use windows_sys::Win32::Foundation::*;
use crate::HidError;

pub type WinResult<T> = Result<T, WinError>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WinError {
    Config(CONFIGRET),
    Win32(Win32Error),
    BufferTooSmall,
    InvalidDeviceId,
    InvalidDeviceNode,
    NoSuchValue,
    WrongPropertyDataType,
    UnexpectedReturnSize,
    InvalidPreparsedData
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

pub fn check_boolean(ret: BOOLEAN) -> WinResult<()> {
    if ret == 0 {
        Err(Win32Error::last().into())
    } else {
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Win32Error {
    Generic(WIN32_ERROR),
    Success,
    IoPending
}

impl Win32Error {

    pub fn last() -> Self {
        match unsafe { GetLastError() }  {
            NO_ERROR => Self::Success,
            ERROR_IO_PENDING => Self::IoPending,
            code => Self::Generic(code)
        }
    }

}

impl From<Win32Error> for WinError {
    fn from(value: Win32Error) -> Self {
        Self::Win32(value)
    }
}