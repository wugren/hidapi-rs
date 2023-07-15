use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::addr_of;
use crate::ensure;
use crate::windows_native::error::{WinError, WinResult};
use crate::windows_native::hid::PreparsedData;


type Usage = u16;

#[derive(Copy, Clone)]
#[repr(C)]
struct LinkCollectionNode {
    link_usage: Usage,
    link_usage_page: Usage,
    parent: u16,
    number_of_children: u16,
    next_sibling: u16,
    first_child: u16,
    type_alias_reserved: u32
}

#[derive(Copy, Clone)]
#[repr(C)]
struct HidCapsInfo {
    first_cap: u16,
    number_of_caps: u16,
    last_cap: u16,
    report_byte_length: u16
}

#[derive(Copy, Clone)]
#[repr(C)]
struct HidpPreparsedData {
    magic_key: [u8; 8],
    usage: Usage,
    usage_page: Usage,
    _reserved: [u16; 2],
    caps_info: [HidCapsInfo; 3],
    first_byte_of_link_collection_array: u16,
    number_link_collection_nodes: u16,
}

const INVALID_DATA: WinResult<usize> = Err(WinError::InvalidPreparsedData);

pub fn get_descriptor(pp_data: &PreparsedData, buf: &mut [u8]) -> WinResult<usize> {
    let mut out = buf;
    unsafe {
        let header: *const HidpPreparsedData = pp_data.as_ptr() as _;
        // Check if MagicKey is correct, to ensure that pp_data points to an valid preparse data structure
        ensure!(&(*header).magic_key == b"HidP KDR", INVALID_DATA);
        // Set pointer to the first node of link_collection_nodes
        let link_collection_nodes: *const LinkCollectionNode = ((addr_of!((*header).caps_info[0]) as *const c_void).offset((*header).first_byte_of_link_collection_array as isize)) as _;

        // TODO Implement the rest
        // https://github.com/libusb/hidapi/blob/d0856c05cecbb1522c24fd2f1ed1e144b001f349/windows/hidapi_descriptor_reconstruct.c#L199
    }
    Ok(0)
}

