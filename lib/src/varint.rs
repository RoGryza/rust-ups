pub fn read_bytes(buf: &mut &[u8]) -> Option<usize> {
    let mut varint = 0;
    let mut shift = 0;
    loop {
        let (c, next_buf) = match buf.split_first() {
            Some(s) => s,
            None => return None,
        };
        *buf = next_buf;
        if c & 0x80 != 0 {
            varint = varint_add_shifted(varint, c & 0x7f, shift)?;
            break;
        }
        varint = varint_add_shifted(varint, c | 0x80, shift)?;
        shift += 7;
    }
    Some(varint)
}

/// Returns `current + x << shift` checking for overflow.
#[inline]
fn varint_add_shifted(current: usize, x: u8, shift: u32) -> Option<usize> {
    (x as usize)
        .checked_shl(shift)
        .and_then(|x2| current.checked_add(x2))
}

pub fn write_bytes(buf: &mut Vec<u8>, mut varint: usize) {
    loop {
        let x = (varint & 0x7f) as u8;
        varint >>= 7;
        if varint == 0 {
            buf.push(x | 0x80);
            break;
        }
        buf.push(x);
        varint -= 1;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_roundtrip(x in any::<usize>()) {
            let serialized = varint_to_vec(x);
            let deserialized = read_bytes(&mut serialized.as_ref()).unwrap();
            prop_assert_eq!(x, deserialized);
        }
    }

    #[test]
    fn test_overflow() {
        let mut serialized = varint_to_vec(usize::MAX);
        // Unset bit flag for last byte and append another one se we go over usize::MAX
        let last = serialized.len() - 1;
        serialized[last] &= 0x7f;
        serialized.push(1);
        assert_eq!(read_bytes(&mut serialized.as_ref()), None);
    }

    fn varint_to_vec(varint: usize) -> Vec<u8> {
        let mut result = Vec::new();
        write_bytes(&mut result, varint);
        result
    }
}
