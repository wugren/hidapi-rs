use std::ffi::{c_void, CString};
use std::mem::{size_of, zeroed};
use bytemuck::cast_slice;
use windows_sys::core::PCWSTR;
use windows_sys::Win32::Devices::HumanInterfaceDevice::{HidD_GetManufacturerString, HidD_GetProductString, HidD_GetSerialNumberString};
use windows_sys::Win32::Devices::Properties::{DEVPKEY_Device_CompatibleIds, DEVPKEY_Device_HardwareIds, DEVPKEY_Device_InstanceId, DEVPKEY_Device_Manufacturer, DEVPKEY_NAME, DEVPROP_TYPE_STRING};
use windows_sys::Win32::Foundation::{BOOLEAN, HANDLE};
use windows_sys::Win32::Storage::EnhancedStorage::{PKEY_DeviceInterface_Bluetooth_DeviceAddress, PKEY_DeviceInterface_Bluetooth_Manufacturer, PKEY_DeviceInterface_Bluetooth_ModelNumber};
use windows_sys::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
use crate::{BusType, DeviceInfo, WcharString};
use crate::windows_native::hid::{get_hid_attributes, get_hid_caps};
use crate::windows_native::interfaces::get_device_interface_property;
use crate::windows_native::string_utils::{extract_int_token_value, starts_with_ignore_case};
use crate::windows_native::types::{DevNode, InternalBuyType, U16Str, U16String, U16StringList};

fn read_string(func: unsafe extern "system" fn (HANDLE, *mut c_void, u32) -> BOOLEAN, handle: HANDLE) -> WcharString {
    //Return empty string on failure to match the c implementation
    let mut string = [0u16; 256];
    if unsafe { func(handle, string.as_mut_ptr() as _, (size_of::<u16>() * string.len()) as u32) } != 0 {
        U16Str::from_slice_list(&string)
            .map(WcharString::from)
            .next()
            .unwrap_or_else(|| WcharString::String(String::new()))
    } else {
        //WcharString::None
        WcharString::String(String::new())
    }
}

pub fn get_device_info(path: &U16Str, handle: HANDLE) -> DeviceInfo {
    let attrib = get_hid_attributes(handle);
    let caps = get_hid_caps(handle)
        .unwrap_or(unsafe { zeroed() });
    let mut dev = DeviceInfo {
        path: CString::new(path.to_string()).unwrap(),
        vendor_id: attrib.VendorID,
        product_id: attrib.ProductID,
        serial_number: read_string(HidD_GetSerialNumberString, handle),
        release_number: attrib.VersionNumber,
        manufacturer_string: read_string(HidD_GetManufacturerString, handle),
        product_string: read_string(HidD_GetProductString, handle),
        usage_page: caps.UsagePage,
        usage: caps.Usage,
        interface_number: -1,
        bus_type: BusType::Unknown,
    };

    get_internal_info(path.as_ptr(), &mut dev);
    dev
}

fn get_internal_info(interface_path: PCWSTR, dev: &mut DeviceInfo) -> Option<()> {
    let device_id = get_device_interface_property(interface_path, &DEVPKEY_Device_InstanceId, DEVPROP_TYPE_STRING)?;
    let device_id = U16Str::from_slice(cast_slice(&device_id));

    let dev_node = DevNode::from_device_id(device_id).ok()?.parent().ok()?;

    let compatible_ids: U16StringList = dev_node.get_property(&DEVPKEY_Device_CompatibleIds)
        .map_err(|err| println!("err: {:?}", err))
        .ok()?;

    let bus_type = compatible_ids
        .iter()
        .filter_map(|compatible_id| match compatible_id.as_slice() {
            /* USB devices
		   https://docs.microsoft.com/windows-hardware/drivers/hid/plug-and-play-support
		   https://docs.microsoft.com/windows-hardware/drivers/install/standard-usb-identifiers */
            id if starts_with_ignore_case(id, "USB") => Some(InternalBuyType::Usb),
            /* Bluetooth devices
		   https://docs.microsoft.com/windows-hardware/drivers/bluetooth/installing-a-bluetooth-device */
            id if starts_with_ignore_case(id, "BTHENUM") => Some(InternalBuyType::Bluetooth),
            id if starts_with_ignore_case(id, "BTHLEDEVICE") => Some(InternalBuyType::BluetoothLE),
            /* I2C devices
		   https://docs.microsoft.com/windows-hardware/drivers/hid/plug-and-play-support-and-power-management */
            id if starts_with_ignore_case(id, "PNP0C50") => Some(InternalBuyType::I2c),
            /* SPI devices
		   https://docs.microsoft.com/windows-hardware/drivers/hid/plug-and-play-for-spi */
            id if starts_with_ignore_case(id, "PNP0C51") => Some(InternalBuyType::Spi),
            _ => None
        })
        .next()
        .unwrap_or(InternalBuyType::Unknown);
    dev.bus_type = bus_type.into();
    match bus_type {
        InternalBuyType::Usb => get_usb_info(dev, dev_node),
        InternalBuyType::BluetoothLE => get_ble_info(dev, dev_node),
        _ => None
    };

    Some(())
}

fn get_usb_info(dev: &mut DeviceInfo, mut dev_node: DevNode) -> Option<()> {
    let mut device_id: U16String = dev_node.get_property(&DEVPKEY_Device_InstanceId).ok()?;

    device_id.make_uppercase_ascii();
    /* Check for Xbox Common Controller class (XUSB) device.
	   https://docs.microsoft.com/windows/win32/xinput/directinput-and-xusb-devices
	   https://docs.microsoft.com/windows/win32/xinput/xinput-and-directinput
	*/
    if extract_int_token_value(device_id.as_slice(), "IG_").is_some() {
        dev_node = dev_node.parent().ok()?;
    }

    let mut hardware_ids: U16StringList = dev_node.get_property(&DEVPKEY_Device_HardwareIds)
        .map_err(|err| println!("err: {:?}", err))
        .ok()?;

    /* Get additional information from USB device's Hardware ID
	   https://docs.microsoft.com/windows-hardware/drivers/install/standard-usb-identifiers
	   https://docs.microsoft.com/windows-hardware/drivers/usbcon/enumeration-of-interfaces-not-grouped-in-collections
	*/
    for hardware_id in hardware_ids.iter_mut() {
        hardware_id.make_uppercase_ascii();

        if dev.release_number == 0 {
            if let Some(release_number) = extract_int_token_value(hardware_id.as_slice(), "REV_") {
                dev.release_number = release_number as u16;
            }
        }
        if dev.interface_number == -1 {
            if let Some(interface_number) = extract_int_token_value(hardware_id.as_slice(), "MI_") {
                dev.interface_number = interface_number as i32;
            }
        }
    }

    /* Try to get USB device manufacturer string if not provided by HidD_GetManufacturerString. */
    if dev.manufacturer_string().map_or(true, str::is_empty) {
        if let Ok(manufacturer_string) = dev_node.get_property::<U16String>(&DEVPKEY_Device_Manufacturer) {
            dev.manufacturer_string = (&*manufacturer_string).into();
        }
    }

    /* Try to get USB device serial number if not provided by HidD_GetSerialNumberString. */
    if dev.serial_number().map_or(true, str::is_empty) {
        let mut usb_dev_node = dev_node;
        if dev.interface_number != -1 {
            /* Get devnode parent to reach out composite parent USB device.
               https://docs.microsoft.com/windows-hardware/drivers/usbcon/enumeration-of-the-composite-parent-device
            */
            usb_dev_node = dev_node.parent().ok()?;
        }

        let device_id: U16String = usb_dev_node.get_property(&DEVPKEY_Device_InstanceId).ok()?;

        /* Extract substring after last '\\' of Instance ID.
		   For USB devices it may contain device's serial number.
		   https://docs.microsoft.com/windows-hardware/drivers/install/instance-ids
		*/
        if let Some(start) = device_id
            .as_slice()
            .rsplit(|c| *c != b'&' as u16)
            .next()
            .and_then(|s| s.iter().rposition(|c| *c != b'\\' as u16)) {
            dev.serial_number = U16Str::from_slice(&device_id.as_slice()[(start + 1)..]).into();
        }

    }

    if dev.interface_number == -1 {
        dev.interface_number = 0;
    }

    Some(())
}

/* HidD_GetProductString/HidD_GetManufacturerString/HidD_GetSerialNumberString is not working for BLE HID devices
   Request this info via dev node properties instead.
   https://docs.microsoft.com/answers/questions/401236/hidd-getproductstring-with-ble-hid-device.html
*/
fn get_ble_info(dev: &mut DeviceInfo, dev_node: DevNode) -> Option<()>{
    if dev.manufacturer_string().map_or(true, str::is_empty) {
        if let Ok(manufacturer_string) = dev_node.get_property::<U16String>(
            (&PKEY_DeviceInterface_Bluetooth_Manufacturer as *const PROPERTYKEY) as _) {
            dev.manufacturer_string = manufacturer_string.into();
        }
    }

    if dev.serial_number().map_or(true, str::is_empty) {
        if let Ok(serial_number) = dev_node.get_property::<U16String>(
            (&PKEY_DeviceInterface_Bluetooth_DeviceAddress as *const PROPERTYKEY) as _) {
            dev.serial_number = serial_number.into();
        }
    }

    if dev.product_string().map_or(true, str::is_empty) {
        let product_string = dev_node.get_property::<U16String>(
            (&PKEY_DeviceInterface_Bluetooth_ModelNumber as *const PROPERTYKEY) as _
        ).or_else(|_| {
            /* Fallback: Get devnode grandparent to reach out Bluetooth LE device node */
            dev_node.parent()
                .and_then(|parent_dev_node| parent_dev_node.get_property(&DEVPKEY_NAME))
        });
        if let Ok(product_string) = product_string {
            dev.product_string = product_string.into();
        }
    }

    Some(())
}
