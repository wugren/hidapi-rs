extern crate hidapi_rust;

use hidapi_rust::HidApi;

fn main() {
    println!("Printing all available hid devices. \nIf some of them are missing and you use \
            linux, make sure, that the device is associated with the hidraw kernel module. This \
             can be done with a proper udev configuration.");
    let api = HidApi::new().unwrap();
    let devices = api.enumerate_info();
    for dev in devices {
        println!("\n{:#?}", dev);
    }
}
