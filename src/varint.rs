pub fn read_bytes(buf: &mut &[u8]) -> Option<usize> {
    let mut varint = 0;
    let mut shift = 0;
    let mut cursor = *buf;
    loop {
        let (c, next_cursor) = match cursor.split_first() {
            Some(s) => s,
            None => return None,
        };
        cursor = next_cursor;
        if c & 0x80 != 0 {
            varint = varint_add_shifted(varint, c & 0x7f, shift)?;
            break;
        }
        varint = varint_add_shifted(varint, c | 0x80, shift)?;
        shift += 7;
    }
    *buf = cursor;
    Some(varint)
}

/// Returns `current + x << shift` checking for overflow.
#[inline]
fn varint_add_shifted(current: usize, x: u8, shift: u32) -> Option<usize> {
    (x as usize)
        .checked_shl(shift)
        .and_then(|x2| current.checked_add(x2))
}

#[cfg(test)]
pub fn to_vec(mut varint: usize) -> Vec<u8> {
    let mut result = Vec::new();
    loop {
        let x = (varint & 0x7f) as u8;
        varint = varint >> 7;
        if varint == 0 {
            result.push(x | 0x80);
            break;
        }
        result.push(x);
        varint -= 1;
    }
    result
}

#[cfg(test)]
mod test {
    use super::*;

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_roundtrip(x in any::<usize>()) {
            let serialized = to_vec(x);
            let deserialized = read_bytes(&mut serialized.as_ref()).unwrap();
            prop_assert_eq!(x, deserialized);
        }
    }

    #[test]
    fn test_overflow() {
        let mut serialized = to_vec(usize::MAX);
        // Unset bit flag for last byte and append another one se we go over usize::MAX
        let last = serialized.len() - 1;
        serialized[last] &= 0x7f;
        serialized.push(1);
        assert_eq!(read_bytes(&mut serialized.as_ref()), None);
    }
}
