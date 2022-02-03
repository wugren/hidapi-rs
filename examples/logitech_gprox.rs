/****************************************************************************
    Copyright (c) 2022 ruabmbua All Rights Reserved.
****************************************************************************/

//! Sets the sidechannel volume of the logitech gpro x headset

extern crate hidapi;

use hidapi::HidApi;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut vol: u8 = std::env::args()
        .nth(1)
        .map(|arg| arg.parse())
        .ok_or("missing sidechannel volume arg")??;

    if vol > 100 {
        vol = 100;
    }

    let api = HidApi::new()?;
    let dev = api.open(0x046d, 0x0aaa)?;

    println!("Setting sidechannel volume to {}", vol);

    dev.write(&[0x11, 0xff, 0x05, 0x1c, vol])?;

    Ok(())
}
