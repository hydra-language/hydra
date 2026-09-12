unsafe extern "C" {
    fn print_str(ptr: *const u8, len: usize);
}

fn encode_char(value: u32, buffer: &mut [u8; 4]) -> &[u8] {
    let c = char::from_u32(value)
        .unwrap_or(char::REPLACEMENT_CHARACTER);

    c.encode_utf8(buffer).as_bytes()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn print_chars(ptr: *const u32, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }

    let chars = unsafe {
        std::slice::from_raw_parts(ptr, len)
    };

    let mut buffer = [0u8; 4];

    for &value in chars {
        let encoded = encode_char(value, &mut buffer);

        unsafe {
            print_str(encoded.as_ptr(), encoded.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::encode_char;

    #[test]
    fn encodes_ascii_char_as_utf8() {
        let mut buffer = [0u8; 4];

        assert_eq!(
            encode_char('A' as u32, &mut buffer),
            "A".as_bytes()
        );
    }

    #[test]
    fn encodes_multibyte_char_as_utf8() {
        let mut buffer = [0u8; 4];

        assert_eq!(
            encode_char('🦀' as u32, &mut buffer),
            "🦀".as_bytes()
        );
    }

    #[test]
    fn invalid_scalar_uses_replacement_character() {
        let mut buffer = [0u8; 4];

        assert_eq!(
            encode_char(0xD800, &mut buffer),
            "�".as_bytes()
        );
    }
}
