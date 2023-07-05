use std::mem::zeroed;
use std::ptr::null;
use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::IO::OVERLAPPED;
use windows_sys::Win32::System::Threading::CreateEventW;
use crate::BusType;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum InternalBuyType {
    Unknown,
    Usb,
    Bluetooth,
    BluetoothLE,
    I2c,
    Spi,
}

impl From<InternalBuyType> for BusType {
    fn from(value: InternalBuyType) -> Self {
        match value {
            InternalBuyType::Unknown => BusType::Unknown,
            InternalBuyType::Usb => BusType::Usb,
            InternalBuyType::Bluetooth => BusType::Bluetooth,
            InternalBuyType::BluetoothLE => BusType::Bluetooth,
            InternalBuyType::I2c => BusType::I2c,
            InternalBuyType::Spi => BusType::Spi
        }
    }
}

pub struct Handle(HANDLE);

impl Handle {
    pub fn from_raw(handle: HANDLE) -> Self {
        Self(handle)
    }
    pub fn as_raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
        self.0 = INVALID_HANDLE_VALUE;
    }
}


pub struct Overlapped(OVERLAPPED);

impl Overlapped {
    pub fn event_handle(&self) -> HANDLE {
        self.0.hEvent
    }
    pub fn as_raw(&mut self) -> *mut OVERLAPPED {
        &mut self.0
    }
}

unsafe impl Send for Overlapped { }

impl Default for Overlapped {
    fn default() -> Self {
        Overlapped(unsafe {
            OVERLAPPED {
                //todo check if event is null
                hEvent: CreateEventW(null(), FALSE, FALSE, null()),
                ..zeroed()
            }
        })
    }
}

impl Drop for Overlapped {
    fn drop(&mut self) {
        if self.0.hEvent != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0.hEvent);
            }
        }
        self.0.hEvent = INVALID_HANDLE_VALUE;
    }
}
