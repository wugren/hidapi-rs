use crate::WcharString;

pub(crate) fn to_upper(u16str: &mut [u16]) {
    for c in u16str {
        if let Ok(t) = u8::try_from(*c) {
            *c = t.to_ascii_uppercase().into();
        }
    }
}

pub(crate) fn find_first_upper_case(u16str: &[u16], pattern: &str) -> Option<usize> {
    u16str
        .windows(pattern.encode_utf16().count())
        .enumerate()
        .filter(|(_, ss)| ss
            .iter()
            .copied()
            .zip(pattern.encode_utf16())
            .all(|(l, r)| l == r))
        .map(|(i, _)| i)
        .next()
}

pub(crate) fn starts_with_ignore_case(utf16str: &[u16], pattern: &str) -> bool {
    //The hidapi c library uses `contains` instead of `starts_with`,
    // but as far as I can tell `starts_with` is a better choice
    char::decode_utf16(utf16str.iter().copied())
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .zip(pattern.chars())
        .all(|(l, r)| l.eq_ignore_ascii_case(&r))
}

pub(crate) fn extract_int_token_value(u16str: &[u16], token: &str) -> Option<u32> {
    let start = find_first_upper_case(u16str, token)? + token.encode_utf16().count();
    char::decode_utf16(u16str[start..].iter().copied())
        .map_while(|c| c
            .ok()
            .and_then(|c| c.to_digit(16)))
        .reduce(|l, r| l * 16 + r)
}

pub(crate) fn u16str_to_wstring(u16str: &[u16]) -> WcharString {
    String::from_utf16(u16str)
        .map(WcharString::String)
        .unwrap_or_else(|_| WcharString::Raw(u16str.to_vec()))
}