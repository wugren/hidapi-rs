/****************************************************************************
    Copyright (c) 2015 Roland Ruckerbauer All Rights Reserved.

    This file is part of hidapi_rust.

    hidapi_rust is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    hidapi_rust is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with hidapi_rust.  If not, see <http://www.gnu.org/licenses/>.
****************************************************************************/

extern crate libc;

mod ffi;

use std::sync::{ONCE_INIT, Once};

static mut INIT: Once = ONCE_INIT;

#[inline(always)]
unsafe fn init() {
    INIT.call_once(||{
        ffi::hid_init();
    });
}

pub struct HidDevice {
    _c_struct: *mut ffi::HidDevice,
}

impl HidDevice {

}
