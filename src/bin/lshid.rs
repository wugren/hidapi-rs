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

extern crate hidapi;
extern crate libc;


fn main() {
    println!("Printing all available hid devices.");

    /*
    unsafe {

        hid_init();

        let devices_root = hid_enumerate(0, 0);
        
        let mut next = &*devices_root;

        while !next.next.is_null() {
            let mut serial_number = "No number found".to_string();

            match wchar_to_string(next.serial_number) {
                Ok(t) => serial_number = t,
                _ => ()
            }

            //Removes mutability from serial_number
            let serial_number = serial_number;

            println!("{}", serial_number);
            next = &*next.next;
        }


        hid_free_enumeration(devices_root);
        hid_exit();

    }
    */

    /*
    let api = HidApi::new().unwrap();
    let devices = api.devices();
    for dev in devices {
        println!("\n{:#?}", dev);
    }
    */
}