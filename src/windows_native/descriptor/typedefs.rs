
pub type Usage = u16;


#[derive(Copy, Clone)]
#[repr(C)]
pub struct LinkCollectionNode {
    pub link_usage: Usage,
    pub link_usage_page: Usage,
    pub parent: u16,
    pub number_of_children: u16,
    pub next_sibling: u16,
    pub first_child: u16,
    pub bits: u32
}

impl LinkCollectionNode {
    pub fn is_alias(&self) -> bool {
        self.bits & 1u32 << 23 != 0
    }
    pub fn collection_type(&self) -> u8 {
        self.bits.to_be_bytes()[0]
    }
}

//Size checked
#[derive(Copy, Clone)]
#[repr(C)]
pub struct CapsInfo {
    pub first_cap: u16,
    pub number_of_caps: u16,
    pub last_cap: u16,
    pub report_byte_length: u16
}

//Size checked
#[derive(Copy, Clone)]
#[repr(C)]
pub struct UnknownToken {
    pub token: u8,
    _reserved: [u8; 3],
    pub bit_field: u32
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct Button {
    pub logical_min: i32,
    pub logical_max: i32
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct NotButton {
    pub has_nul: u8,
    _reserved: [u8; 3],
    pub logical_min: i32,
    pub logical_max: i32,
    pub physical_min: i32,
    pub physical_max: i32
}

#[derive(Copy, Clone)]
#[repr(C)]
pub union MaybeButton {
    button: Button,
    not_button: NotButton
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct Range {
    pub usage_mix: Usage,
    pub usage_max: Usage,
    pub string_min: u16,
    pub string_max: u16,
    pub designator_min: u16,
    pub designator_max: u16,
    pub data_index_min: u16,
    pub data_index_max: u16
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct NotRange {
    pub usage: Usage,
    _reserved1: Usage,
    pub string_index: u16,
    _reserved2: u16,
    pub designator_index: u16,
    _reserved3: u16,
    pub data_index: u16,
    _reserved4: u16,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub union MaybeRange {
    range: Range,
    not_range: NotRange
}


//Size checked
#[derive(Copy, Clone)]
#[repr(C)]
pub struct Caps {
    pub usage_page: Usage,
    pub report_id: u8,
    pub bit_position: u8,
    pub report_size: u16,
    pub report_count: u16,
    pub byte_position: u16,
    pub bit_count: u16,
    pub bit_field: u32,
    pub next_byte_position: u16,
    pub link_collection: u16,
    pub link_usage_page: Usage,
    pub link_usage: Usage,
    pub flags: u8,
    _reserved: [u8; 3],
    pub unknown_tokens: [UnknownToken; 4],
    pub maybe_range: MaybeRange,
    pub maybe_button: MaybeButton,
    pub units: u32,
    pub units_exp: u32
}

impl Caps {
    pub fn is_alias(&self) -> bool {
        self.flags & (1 << 2) != 0
    }
    pub fn is_button_cap(&self) -> bool {
        self.flags & (1 << 5) != 0
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct HidpPreparsedData {
    pub magic_key: [u8; 8],
    pub usage: Usage,
    pub usage_page: Usage,
    _reserved: [u16; 2],
    pub caps_info: [CapsInfo; 3],
    pub first_byte_of_link_collection_array: u16,
    pub number_link_collection_nodes: u16,
    pub caps: [Caps; 3]
}

#[cfg(test)]
mod test {
    use std::mem::size_of;
    use super::*;

    #[test]
    fn test_struct_sizes() {
        assert_eq!(size_of::<Caps>(), 104);
        assert_eq!(size_of::<UnknownToken>(), 8);
        assert_eq!(size_of::<CapsInfo>(), 8);
        assert_eq!(size_of::<LinkCollectionNode>(), 16);
    }

}