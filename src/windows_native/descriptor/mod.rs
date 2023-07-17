mod typedefs;
mod types;

use std::collections::HashMap;
use std::ffi::c_void;
use std::iter::once;
use std::ops::DerefMut;
use std::ptr::addr_of;
use std::rc::Rc;
use std::slice;
use crate::ensure;
use crate::windows_native::descriptor::typedefs::{HidpPreparsedData, LinkCollectionNode};
use crate::windows_native::descriptor::types::{BitRange, ItemNodeType, Items, MainItemNode, MainItems, ReportType};
use crate::windows_native::error::{WinError, WinResult};
use crate::windows_native::hid::PreparsedData;



const INVALID_DATA: WinResult<Vec<u8>> = Err(WinError::InvalidPreparsedData);

pub fn get_descriptor(pp_data: &PreparsedData) -> WinResult<Vec<u8>> {
    unsafe {
        let header: *const HidpPreparsedData = pp_data.as_ptr() as _;
        // Check if MagicKey is correct, to ensure that pp_data points to an valid preparse data structure
        ensure!(&(*header).magic_key == b"HidP KDR", INVALID_DATA);
        // Set pointer to the first node of link_collection_nodes
        let link_collection_nodes = {
            let ptr: *const LinkCollectionNode = ((addr_of!((*header).caps_info[0]) as *const c_void).offset((*header).first_byte_of_link_collection_array as isize)) as _;
            let len = (*header).number_link_collection_nodes as usize;
            slice::from_raw_parts(ptr, len)
        };

        // ****************************************************************************************************************************
        // Create lookup tables for the bit range of each report per collection (position of first bit and last bit in each collection)
        // coll_bit_range[COLLECTION_INDEX][REPORT_ID][INPUT/OUTPUT/FEATURE]
        // ****************************************************************************************************************************
        let mut coll_bit_range: HashMap<(usize, u8, ReportType), BitRange> = HashMap::new();
        for collection_node_idx in 0..link_collection_nodes.len() {
            for reportid_idx in 0..=255 {
                for rt_idx in ReportType::values() {
                    coll_bit_range.insert((collection_node_idx, reportid_idx, rt_idx), BitRange::default());
                }
            }
        }

        for rt_idx in ReportType::values() {
            let caps_info = (*header).caps_info[rt_idx as usize];
            for caps_idx in caps_info.first_cap..caps_info.last_cap {
                let caps = (*header).caps[caps_idx as usize];
                let first_bit = (caps.byte_position - 1) * 8 + caps.bit_position as u16;
                let last_bit = first_bit + caps.report_size * caps.report_count - 1;
                let range = coll_bit_range.get_mut(&(caps.link_collection as usize, caps.report_id, rt_idx)).unwrap();
                range.first_bit = range.first_bit.into_iter().chain(once(first_bit)).min();
                range.last_bit = range.last_bit.into_iter().chain(once(last_bit)).max();
            }
        }

        // *************************************************************************
        // -Determine hierachy levels of each collections and store it in:
        //  coll_levels[COLLECTION_INDEX]
        // -Determine number of direct childs of each collections and store it in:
        //  coll_number_of_direct_childs[COLLECTION_INDEX]
        // *************************************************************************
        let mut max_coll_level = 0;
        let mut coll_levels = Vec::new();
        let mut coll_number_of_direct_childs = Vec::new();
        for _ in 0..((*header).number_link_collection_nodes) {
            coll_levels.push(-1);
            coll_number_of_direct_childs.push(0);
        }

        {
            let mut actual_coll_level = 0;
            let mut collection_node_idx = 0;
            while actual_coll_level >= 0 {
                coll_levels[collection_node_idx] = actual_coll_level;
                let node = link_collection_nodes[collection_node_idx];
                if node.number_of_children > 0 && coll_levels[node.first_child as usize] == -1 {
                    actual_coll_level += 1;
                    coll_levels[collection_node_idx] = actual_coll_level;
                    max_coll_level = max_coll_level.max(actual_coll_level);
                    coll_number_of_direct_childs[collection_node_idx] += 1;
                    collection_node_idx = node.first_child as usize;
                } else if node.next_sibling != 0 {
                    coll_number_of_direct_childs[node.parent as usize] += 1;
                    collection_node_idx = node.next_sibling as usize;
                } else {
                    actual_coll_level -= 1;
                    if actual_coll_level >= 0 {
                        collection_node_idx = node.parent as usize;
                    }
                }
            }
        }

        // *********************************************************************************
        // Propagate the bit range of each report from the child collections to their parent
        // and store the merged result for the parent
        // *********************************************************************************
        for actual_coll_level in (0..max_coll_level).rev() {
            for collection_node_idx in 0..link_collection_nodes.len() {
                if coll_levels[collection_node_idx] == actual_coll_level {
                    let mut child_idx = link_collection_nodes[collection_node_idx].first_child as usize;
                    while child_idx != 0 {
                        for reportid_idx in 0..=255 {
                            for rt_idx in ReportType::values() {
                                let child = coll_bit_range
                                    .get(&(child_idx, reportid_idx, rt_idx))
                                    .unwrap()
                                    .clone();
                                let parent = coll_bit_range
                                    .get_mut(&(collection_node_idx, reportid_idx, rt_idx))
                                    .unwrap();
                                parent.first_bit = parent.first_bit.into_iter().chain(child.first_bit).min();
                                parent.last_bit = parent.last_bit.into_iter().chain(child.last_bit).max();
                                child_idx = link_collection_nodes[child_idx as usize].next_sibling as usize;
                            }
                        }
                    }
                }
            }
        }

        // *************************************************************************************************
        // Determine child collection order of the whole hierachy, based on previously determined bit ranges
        // and store it this index coll_child_order[COLLECTION_INDEX][DIRECT_CHILD_INDEX]
        // *************************************************************************************************
        let mut coll_child_order: HashMap<(usize, u16), usize> = HashMap::new();
        {
            let mut coll_parsed_flag = vec![false; link_collection_nodes.len()];
            let mut actual_coll_level = 0;
            let mut collection_node_idx = 0;
            while actual_coll_level >= 0 {
                if coll_number_of_direct_childs[collection_node_idx] != 0 &&
                    !coll_parsed_flag[link_collection_nodes[collection_node_idx].first_child as usize] {
                    coll_parsed_flag[link_collection_nodes[collection_node_idx].first_child as usize] = true;

                    {
                        // Create list of child collection indices
                        // sorted reverse to the order returned to HidP_GetLinkCollectionNodeschild
                        // which seems to match teh original order, as long as no bit position needs to be considered
                        let mut child_idx = link_collection_nodes[collection_node_idx].first_child as usize;
                        let mut child_count = coll_number_of_direct_childs[collection_node_idx] - 1;
                        coll_child_order.insert((collection_node_idx, child_count as u16), child_idx);
                        while link_collection_nodes[child_idx as usize].next_sibling != 0 {
                            child_count -= 1;
                            child_idx = link_collection_nodes[child_idx].next_sibling as usize;
                            coll_child_order.insert((collection_node_idx, child_count as u16), child_idx);
                        }
                    }

                    if coll_number_of_direct_childs[collection_node_idx] > 1 {
                        // Sort child collections indices by bit positions
                        for rt_idx in ReportType::values() {
                            for report_idx in 0..=255 {
                                for child_idx in 1..coll_number_of_direct_childs[collection_node_idx] {
                                    // since the coll_bit_range array is not sorted, we need to reference the collection index in
                                    // our sorted coll_child_order array, and look up the corresponding bit ranges for comparing values to sort
                                    let prev_coll_idx = *coll_child_order
                                        .get(&(collection_node_idx, (child_idx - 1) as u16))
                                        .unwrap();
                                    let cur_coll_idx = *coll_child_order
                                        .get(&(collection_node_idx, child_idx as u16))
                                        .unwrap();
                                    let swap = coll_bit_range
                                        .get(&(prev_coll_idx, report_idx, rt_idx))
                                        .and_then(|prev| prev.first_bit)
                                        .zip(coll_bit_range
                                            .get(&(cur_coll_idx, report_idx, rt_idx))
                                            .and_then(|prev| prev.first_bit))
                                        .map_or(false, |(prev, cur)| prev > cur);
                                    if swap {
                                        coll_child_order.insert((collection_node_idx, (child_idx - 1) as u16), cur_coll_idx);
                                        coll_child_order.insert((collection_node_idx, child_idx as u16), prev_coll_idx);
                                    }
                                }
                            }
                        }
                    }
                    actual_coll_level += 1;
                    collection_node_idx = link_collection_nodes[collection_node_idx].first_child as usize;
                } else if link_collection_nodes[collection_node_idx].next_sibling != 0 {
                    collection_node_idx = link_collection_nodes[collection_node_idx].next_sibling as usize;
                } else {
                    actual_coll_level -= 1;
                    if actual_coll_level >= 0 {
                        collection_node_idx = link_collection_nodes[collection_node_idx].parent as usize;
                    }
                }
            }
        }

        // ***************************************************************************************
        // Create sorted main_item_list containing all the Collection and CollectionEnd main items
        // ***************************************************************************************
        let mut main_item_list: Option<Rc<MainItemNode>> = None;
        // Lookup table to find the Collection items in the list by index
        let mut coll_begin_lookup = HashMap::new();
        let mut coll_end_lookup = HashMap::new();
        {
            let mut coll_last_written_child  = vec![-1i32; link_collection_nodes.len()];

            let mut actual_coll_level = 0;
            let mut collection_node_idx = 0;
            let mut first_delimiter_node = None;
            let mut delimiter_close_node = None;
            coll_begin_lookup.insert(0,
                append_main_item_node(
                    MainItemNode::new(0, 0, ItemNodeType::Collection, 0, collection_node_idx, MainItems::Collection, 0),
                    &mut main_item_list
                ));
            while actual_coll_level >= 0 {
                if coll_number_of_direct_childs[collection_node_idx] != 0 && coll_last_written_child[collection_node_idx] == -1 {
                    // Collection has child collections, but none is written to the list yet
                    coll_last_written_child[collection_node_idx] = coll_child_order[&(collection_node_idx, 0)] as i32;
                    collection_node_idx = coll_child_order[&(collection_node_idx, 0)];

                    // In a HID Report Descriptor, the first usage declared is the most preferred usage for the control.
                    // While the order in the WIN32 capabiliy strutures is the opposite:
                    // Here the preferred usage is the last aliased usage in the sequence.
                    if link_collection_nodes[collection_node_idx].is_alias() && first_delimiter_node.is_none() {
                        first_delimiter_node = main_item_list.clone();
                        coll_begin_lookup.insert(collection_node_idx, append_main_item_node(
                            MainItemNode::new(0, 0, ItemNodeType::Collection, 0, collection_node_idx, MainItems::DelimiterUsage, 0),
                            &mut main_item_list));
                        coll_begin_lookup.insert(collection_node_idx, append_main_item_node(
                            MainItemNode::new(0, 0, ItemNodeType::Collection, 0, collection_node_idx, MainItems::DelimiterClose, 0),
                            &mut main_item_list));
                        delimiter_close_node = main_item_list.clone();
                    } else {
                        coll_begin_lookup.insert(collection_node_idx, append_main_item_node(
                            MainItemNode::new(0, 0, ItemNodeType::Collection, 0, collection_node_idx, MainItems::Collection, 0),
                            &mut main_item_list));
                        actual_coll_level += 1;
                    }
                } else if coll_number_of_direct_childs[collection_node_idx] > 1 &&
                    coll_last_written_child[collection_node_idx] != coll_child_order[&(collection_node_idx, (coll_number_of_direct_childs[collection_node_idx] - 1) as u16)] as i32 {

                    // Collection has child collections, and this is not the first child
                    let mut next_child = 1;
                    while coll_last_written_child[collection_node_idx] != coll_child_order[&(collection_node_idx, (next_child - 1))] as i32 {
                        next_child += 1;
                    }
                    coll_last_written_child[collection_node_idx] = coll_child_order[&(collection_node_idx, next_child)] as i32;
                    collection_node_idx = coll_child_order[&(collection_node_idx, next_child)];

                    if link_collection_nodes[collection_node_idx].is_alias() && first_delimiter_node.is_none() {
                        // Alliased Collection (First node in link_collection_nodes -> Last entry in report descriptor output)
                        first_delimiter_node = main_item_list.clone();
                        coll_begin_lookup.insert(collection_node_idx, append_main_item_node(
                            MainItemNode::new(0, 0, ItemNodeType::Collection, 0, collection_node_idx, MainItems::DelimiterUsage, 0),
                            &mut main_item_list));
                        coll_begin_lookup.insert(collection_node_idx, append_main_item_node(
                            MainItemNode::new(0, 0, ItemNodeType::Collection, 0, collection_node_idx, MainItems::DelimiterClose, 0),
                            &mut main_item_list));
                        delimiter_close_node = main_item_list.clone();
                    } else if link_collection_nodes[collection_node_idx].is_alias() && first_delimiter_node.is_some() {
                        coll_begin_lookup.insert(collection_node_idx, append_main_item_node(
                            MainItemNode::new(0, 0, ItemNodeType::Collection, 0, collection_node_idx, MainItems::DelimiterUsage, 0),
                            &mut first_delimiter_node));
                    } else if !link_collection_nodes[collection_node_idx].is_alias() && first_delimiter_node.is_some() {
                        coll_begin_lookup.insert(collection_node_idx, append_main_item_node(
                            MainItemNode::new(0, 0, ItemNodeType::Collection, 0, collection_node_idx, MainItems::DelimiterUsage, 0),
                            &mut first_delimiter_node));
                        coll_begin_lookup.insert(collection_node_idx, append_main_item_node(
                            MainItemNode::new(0, 0, ItemNodeType::Collection, 0, collection_node_idx, MainItems::DelimiterClose, 0),
                            &mut first_delimiter_node));
                        first_delimiter_node = None;
                        main_item_list = delimiter_close_node.take();
                    }
                    if !link_collection_nodes[collection_node_idx].is_alias() {
                        coll_begin_lookup.insert(collection_node_idx, append_main_item_node(
                            MainItemNode::new(0, 0, ItemNodeType::Collection, 0, collection_node_idx, MainItems::Collection, 0),
                            &mut main_item_list));
                        actual_coll_level += 1;
                    }
                } else {
                    actual_coll_level -= 1;
                    coll_end_lookup.insert(collection_node_idx, append_main_item_node(
                        MainItemNode::new(0, 0, ItemNodeType::Collection, 0, collection_node_idx, MainItems::CollectionEnd, 0),
                        &mut main_item_list));
                    collection_node_idx = link_collection_nodes[collection_node_idx].parent as usize;
                }
            }
        }

        // ****************************************************************
        // Inserted Input/Output/Feature main items into the main_item_list
        // in order of reconstructed bit positions
        // ****************************************************************
        for rt_idx in ReportType::values() {
            // Add all value caps to node list
            let mut first_delimiter_node = None;
            let mut delimiter_close_node = None;
            let caps_info = (*header).caps_info[rt_idx as usize];
            for caps_idx in caps_info.first_cap..caps_info.last_cap {
                let caps = (*header).caps[caps_idx as usize];
                let mut coll_begin = coll_begin_lookup[&(caps.link_collection as usize)].clone();
                let first_bit = (caps.byte_position - 1) * 8 + caps.bit_position as u16;
                let last_bit = first_bit + caps.report_size * caps.report_count - 1;

                for child_idx in 0..coll_number_of_direct_childs[caps.link_collection as usize] {
                    // Determine in which section before/between/after child collection the item should be inserted
                    if first_bit < coll_bit_range[&(coll_child_order[&(caps.link_collection as usize, child_idx)], caps.report_id, rt_idx)].first_bit.unwrap_or(0) {
                        // Note, that the default value for undefined coll_bit_range is -1, which can't be greater than the bit position
                        break;
                    }
                    coll_begin = coll_end_lookup[&coll_child_order[&(caps.link_collection as usize, child_idx)]].clone();
                }
                let mut list_node = search_list(first_bit as i32, rt_idx.into(), caps.report_id, coll_begin.clone());

                // In a HID Report Descriptor, the first usage declared is the most preferred usage for the control.
                // While the order in the WIN32 capabiliy strutures is the opposite:
                // Here the preferred usage is the last aliased usage in the sequence.

                if caps.is_alias() && first_delimiter_node.is_none() {
                    // Alliased Usage (First node in pp_data->caps -> Last entry in report descriptor output)
                    first_delimiter_node = Some(list_node.clone());
                    append_main_item_node(
                        MainItemNode::new(first_bit, last_bit, ItemNodeType::Cap, caps_idx as i32, caps.link_collection as usize, MainItems::DelimiterUsage, caps.report_id),
                        &mut Some(list_node.clone())
                    );
                    append_main_item_node(
                        MainItemNode::new(first_bit, last_bit, ItemNodeType::Cap, caps_idx as i32, caps.link_collection as usize, MainItems::DelimiterClose, caps.report_id),
                        &mut Some(list_node.clone())
                    );
                    delimiter_close_node = Some(list_node.clone());
                } else if caps.is_alias() && first_delimiter_node.is_some() {
                    append_main_item_node(
                        MainItemNode::new(first_bit, last_bit, ItemNodeType::Cap, caps_idx as i32, caps.link_collection as usize, MainItems::DelimiterUsage, caps.report_id),
                        &mut Some(list_node.clone())
                    );
                } else if !caps.is_alias() && first_delimiter_node.is_some() {
                    // Alliased Collection (Last node in pp_data->caps -> First entry in report descriptor output)
                    append_main_item_node(
                        MainItemNode::new(first_bit, last_bit, ItemNodeType::Cap, caps_idx as i32, caps.link_collection as usize, MainItems::DelimiterUsage, caps.report_id),
                        &mut Some(list_node.clone())
                    );
                    append_main_item_node(
                        MainItemNode::new(first_bit, last_bit, ItemNodeType::Cap, caps_idx as i32, caps.link_collection as usize, MainItems::DelimiterOpen, caps.report_id),
                        &mut Some(list_node.clone())
                    );
                    first_delimiter_node = None;
                    list_node = delimiter_close_node.take().unwrap();
                }
                if !caps.is_alias() {
                    append_main_item_node(
                        MainItemNode::new(first_bit, last_bit, ItemNodeType::Cap, caps_idx as i32, caps.link_collection as usize, rt_idx.into(), caps.report_id),
                        &mut Some(list_node.clone())
                    );
                }
            }
        }

        // ***********************************************************
        // Add const main items for padding to main_item_list
        // -To fill all bit gaps
        // -At each report end for 8bit padding
        //  Note that information about the padding at the report end,
        //  is not stored in the preparsed data, but in practice all
        //  report descriptors seem to have it, as assumed here.
        // ***********************************************************
        {
            let mut last_bit_position: HashMap<(MainItems, u8), i32> = HashMap::new();
            let mut last_report_item_lookup: HashMap<(MainItems, u8), Rc<MainItemNode>> = HashMap::new();

            let mut list = main_item_list.clone().unwrap();
            while let Some(next) = list.next.get() {
                if let Ok(_) = ReportType::try_from(list.main_item_type) {
                    let lbp = last_bit_position
                        .get(&(list.main_item_type, list.report_id))
                        .cloned()
                        .unwrap_or(-1);
                    let lrip = last_report_item_lookup
                        .get(&(list.main_item_type, list.report_id))
                        .cloned();
                    if lbp + 1 != list.first_bit as i32 && lrip.as_ref()
                        .is_some_and(|i| i.first_bit != list.first_bit) {
                        let list_node = search_list(lbp, list.main_item_type, list.report_id, lrip.unwrap());
                        append_main_item_node(
                            MainItemNode::new((lbp + 1) as u16, list.first_bit - 1, ItemNodeType::Padding, -1, 0, list.main_item_type, list.report_id),
                            &mut Some(list_node)
                        );
                    }
                    last_bit_position.insert((list.main_item_type, list.report_id), list.last_bit as i32);
                    last_report_item_lookup.insert((list.main_item_type, list.report_id), list.clone());
                }
                list = next.clone();
            }
            for rt_idx in ReportType::values() {
                for report_idx in 0..=255 {
                    if let Some(lbp) = last_bit_position.get(&(rt_idx.into(), report_idx)) {
                        let padding = 8 - ((*lbp + 1) % 8);
                        if padding < 8 {
                            // Insert padding item after item referenced in last_report_item_lookup
                            let mut lrip = last_report_item_lookup.get_mut(&(rt_idx.into(), report_idx)).cloned();
                            append_main_item_node(
                                MainItemNode::new((lbp + 1) as u16, (lbp + padding) as u16, ItemNodeType::Padding, -1, 0, rt_idx.into(), report_idx),
                                &mut lrip
                            );
                            if let Some(lrip) = lrip {
                                last_report_item_lookup.insert((rt_idx.into(), report_idx), lrip);
                            }
                        }
                    }
                }
            }
        }

        // ***********************************
        // Encode the report descriptor output
        // ***********************************

        let mut writer = DescriptorWriter::default();

        let mut last_report_id = 0;
        let mut last_usage_page = 0;
        let mut last_physical_min = 0;// If both, Physical Minimum and Physical Maximum are 0, the logical limits should be taken as physical limits according USB HID spec 1.11 chapter 6.2.2.7
        let mut last_physical_max = 0;
        let mut last_unit_exponent = 0; // If Unit Exponent is Undefined it should be considered as 0 according USB HID spec 1.11 chapter 6.2.2.7
        let mut last_unit = 0; // If the first nibble is 7, or second nibble of Unit is 0, the unit is None according USB HID spec 1.11 chapter 6.2.2.7
        let mut inhibit_write_of_usage = false; // Needed in case of delimited usage print, before the normal collection or cap
        let mut report_count = 0;

        while let Some(current) = main_item_list {

            main_item_list = current.next.get().cloned();
        }

        // TODO Implement the rest
        // https://github.com/libusb/hidapi/blob/d0856c05cecbb1522c24fd2f1ed1e144b001f349/windows/hidapi_descriptor_reconstruct.c#L199

        Ok(writer.finish())
    }
}

fn search_list(search_bit: i32, main_item_type: MainItems, report_id: u8, mut list: Rc<MainItemNode>) -> Rc<MainItemNode> {
    loop {
        let next = list.next.get().unwrap().clone();
        if next.main_item_type != MainItems::Collection &&
            next.main_item_type != MainItems::CollectionEnd &&
            !(next.last_bit as i32 >= search_bit && next.report_id == report_id && next.main_item_type == main_item_type) {
            list = next;
        } else {
            break;
        }
    }
    list.clone()
}

fn append_main_item_node(node: MainItemNode, list: &mut Option<Rc<MainItemNode>>) -> Rc<MainItemNode> {
    let rc = Rc::new(node);
    match list {
        None => *list = Some(rc.clone()),
        Some(ref current) => {
            let mut current = current;
            loop {
                match current.next.get() {
                    None => {
                        current.next.set(rc.clone()).unwrap();
                        break;
                    },
                    Some(next) => current = next
                }
            }
        }
    }
    rc
}

#[derive(Default)]
struct DescriptorWriter(Vec<u8>);

impl DescriptorWriter {

    // Writes a short report descriptor item according USB HID spec 1.11 chapter 6.2.2.2
    fn write(&mut self, item: Items, data: impl Into<i64>) -> WinResult<()> {
        let data = data.into();
        match item {
            Items::MainCollectionEnd => {
                self.0.push(item as u8);
            },
            Items::GlobalLogicalMinimum | Items::GlobalLogicalMaximum | Items::GlobalPhysicalMinimum | Items::GlobalPhysicalMaximum => {
                if let Ok(data) = i8::try_from(data) {
                    self.0.push((item as u8) + 0x01);
                    self.0.extend(data.to_le_bytes())
                } else if let Ok(data) = i16::try_from(data) {
                    self.0.push((item as u8) + 0x02);
                    self.0.extend(data.to_le_bytes())
                } else if let Ok(data) = i32::try_from(data) {
                    self.0.push((item as u8) + 0x03);
                    self.0.extend(data.to_le_bytes())
                } else {
                    return Err(WinError::InvalidPreparsedData)
                }
            },
            _=> {
                if let Ok(data) = u8::try_from(data) {
                    self.0.push((item as u8) + 0x01);
                    self.0.extend(data.to_le_bytes())
                } else if let Ok(data) = u16::try_from(data) {
                    self.0.push((item as u8) + 0x02);
                    self.0.extend(data.to_le_bytes())
                } else if let Ok(data) = u32::try_from(data) {
                    self.0.push((item as u8) + 0x03);
                    self.0.extend(data.to_le_bytes())
                } else {
                    return Err(WinError::InvalidPreparsedData)
                }
            }
        }
        Ok(())
    }

    fn finish(self) -> Vec<u8> {
        self.0
    }

}