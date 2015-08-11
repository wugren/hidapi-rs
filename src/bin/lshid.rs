extern crate hidapi_rust;

use hidapi_rust::HidApi;

fn main() {
    println!("Printing all available hid devices.");
    let api = HidApi::new().unwrap();
    let devices = api.enumerate_info();
    for dev in devices {
        println!("\n{:#?}", dev);
    }
}
