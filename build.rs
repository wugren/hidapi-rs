extern crate gcc;
extern crate pkg_config;

fn main() {
    compile();
}

#[cfg(target_os = "linux")]
fn compile() {
    let mut config = gcc::Config::new();
    config.file("etc/hidapi/libusb/hid.c").include("etc/hidapi/hidapi");
    let lib = pkg_config::find_library("libusb-1.0").unwrap();
    for path in lib.include_paths {
        config.include(path.to_str().unwrap());
    }
    config.compile("libhidapi.a");
}
