extern crate hidapi_rust;

use hidapi_rust::HidDeviceInfoEnumeration;

fn main() {
    println!("Hello, world!");
    let devices = HidDeviceInfoEnumeration::new();
    for dev in devices {
        println!("{:#?}\n", dev);
    }
}
