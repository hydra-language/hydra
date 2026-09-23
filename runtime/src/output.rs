unsafe extern "C" {
    fn print_str(ptr: *const u8, len: usize);
}

const INTEGER_BUFFER_SIZE: usize = 20;

fn format_unsigned(mut value: u64, buffer: &mut [u8], start: usize) -> usize {
    if value == 0 {
        buffer[start] = b'0';
        return 1;
    }

    let mut cursor = INTEGER_BUFFER_SIZE;

    while value != 0 {
        cursor -= 1;
        buffer[cursor] = b'0' + (value % 10) as u8;
        value /= 10;
    }

    let len = INTEGER_BUFFER_SIZE - cursor;

    buffer.copy_within(
        cursor..INTEGER_BUFFER_SIZE,
        start,
    );

    len
}

fn format_i64(value: i64, buffer: &mut [u8]) -> usize {
    if value < 0 {
        buffer[0] = b'-';

        return 1 + format_unsigned(
            value.unsigned_abs(),
            buffer,
            1,
        );
    }

    format_unsigned(
        value as u64,
        buffer,
        0,
    )
}

fn format_u64(value: u64, buffer: &mut [u8]) -> usize {
    format_unsigned(
        value,
        buffer,
        0,
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hydra_format_i64(value: i64, ptr: *mut u8) -> usize {
    if ptr.is_null() {
        return 0;
    }

    let buffer = unsafe {
        std::slice::from_raw_parts_mut(
            ptr,
            INTEGER_BUFFER_SIZE,
        )
    };

    format_i64(
        value,
        buffer,
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hydra_format_u64(value: u64, ptr: *mut u8) -> usize {
    if ptr.is_null() {
        return 0;
    }

    let buffer = unsafe {
        std::slice::from_raw_parts_mut(
            ptr,
            INTEGER_BUFFER_SIZE,
        )
    };

    format_u64(
        value,
        buffer,
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hydra_format_char(value: u32, ptr: *mut u8) -> usize {
    if ptr.is_null() {
        return 0;
    }

    let buffer = unsafe {
        &mut *(ptr as *mut [u8; 4])
    };

    let encoded = encode_char(
        value,
        buffer,
    );

    encoded.len()
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
        std::slice::from_raw_parts(
            ptr,
            len,
        )
    };

    let mut buffer = [0u8; 4];

    for &value in chars {
        let encoded = encode_char(
            value,
            &mut buffer,
        );

        unsafe {
            print_str(
                encoded.as_ptr(),
                encoded.len(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        encode_char,
        format_i64,
        format_u64,
        INTEGER_BUFFER_SIZE,
    };

    fn signed(value: i64) -> String {
        let mut buffer = [0u8; INTEGER_BUFFER_SIZE];
        let len = format_i64(value, &mut buffer);

        String::from_utf8(
            buffer[..len].to_vec(),
        )
        .unwrap()
    }

    fn unsigned(value: u64) -> String {
        let mut buffer = [0u8; INTEGER_BUFFER_SIZE];
        let len = format_u64(value, &mut buffer);

        String::from_utf8(
            buffer[..len].to_vec(),
        )
        .unwrap()
    }

    #[test]
    fn formats_signed_decimal_integers() {
        assert_eq!(signed(0), "0");
        assert_eq!(signed(1), "1");
        assert_eq!(signed(-1), "-1");
        assert_eq!(signed(69), "69");
        assert_eq!(signed(-69), "-69");
        assert_eq!(signed(i64::MAX), "9223372036854775807");
        assert_eq!(signed(i64::MIN), "-9223372036854775808");
    }

    #[test]
    fn formats_unsigned_decimal_integers() {
        assert_eq!(unsigned(0), "0");
        assert_eq!(unsigned(1), "1");
        assert_eq!(unsigned(69), "69");
        assert_eq!(unsigned(u64::MAX), "18446744073709551615");
    }

    #[test]
    fn encodes_ascii_char_as_utf8() {
        let mut buffer = [0u8; 4];

        assert_eq!(
            encode_char(
                'A' as u32,
                &mut buffer,
            ),
            "A".as_bytes(),
        );
    }

    #[test]
    fn encodes_multibyte_char_as_utf8() {
        let mut buffer = [0u8; 4];

        assert_eq!(
            encode_char(
                '🦀' as u32,
                &mut buffer,
            ),
            "🦀".as_bytes(),
        );
    }

    #[test]
    fn invalid_scalar_uses_replacement_character() {
        let mut buffer = [0u8; 4];

        assert_eq!(
            encode_char(
                0xD800,
                &mut buffer,
            ),
            "�".as_bytes(),
        );
    }
}
