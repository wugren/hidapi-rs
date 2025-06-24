/****************************************************************************
Copyright (c) 2015 Osspial All Rights Reserved.

This file is part of hidapi-rs, based on hidapi_rust by Roland Ruckerbauer.
****************************************************************************/

//! This example shows the added possibility (after version 0.4.1),
//! to move devices into a function / or closure with static lifetime bounds.

extern crate hidapi;

use hidapi::{HidApi, HidDevice};
use std::rc::Rc;

fn main() {
    let _dev = test_lt();
}

fn requires_static_lt_bound<F: Fn() + 'static>(f: F) {
    f();
}

fn test_lt() -> Rc<HidDevice> {
    let api = HidApi::new().expect("Hidapi init failed");

    let mut devices = api.device_list();

    let dev_info = devices
        .next()
        .expect("There is not a single hid device available");

    let dev = Rc::new(
        HidApi::open(dev_info.vendor_id(), dev_info.product_id())
            .expect("Can not open device"),
    );

    let dev_1 = dev.clone();
    requires_static_lt_bound(move || {
        println!("{:?}", dev_1.get_device_info().unwrap()); //<! Can be captured by closure with static lt
    });

    dev //<! Can be returned from a function, which exceeds the lifetime of the API context
}
