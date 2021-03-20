#[cfg(test)]
use std::io::Write;
use std::io::{self, Read};

pub fn read<R: Read>(reader: R) -> io::Result<(usize, usize)> {
    let mut varint = 0;
    let mut shift = 0;
    for (i, c_res) in reader.bytes().enumerate() {
        let c = c_res?;
        if c & 0x80 != 0 {
            varint = varint_add_shifted(varint, c & 0x7f, shift)?;
            return Ok((varint, i + 1));
        }
        varint = varint_add_shifted(varint, c | 0x80, shift)?;
        shift += 7;
    }

    Err(io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "Unexpected EOF while reading varint",
    ))
}

/// Returns `current + x << shift` checking for overflow.
#[inline]
fn varint_add_shifted(current: usize, x: u8, shift: u32) -> io::Result<usize> {
    (x as usize)
        .checked_shl(shift)
        .and_then(|x2| current.checked_add(x2))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Overflow while reading varint"))
}

#[cfg(test)]
pub fn write<W: Write>(mut writer: W, mut varint: usize) -> io::Result<()> {
    loop {
        let x = (varint & 0x7f) as u8;
        varint = varint >> 7;
        if varint == 0 {
            writer.write_all(&[x | 0x80])?;
            break;
        }
        writer.write_all(&[x])?;
        varint = varint - 1;
    }
    Ok(())
}

#[cfg(test)]
pub fn to_vec(varint: usize) -> Vec<u8> {
    let mut result = Vec::new();
    match write(&mut result, varint) {
        Ok(_) => result,
        Err(_) => std::unreachable!(),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::{Cursor, ErrorKind};

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_roundtrip(x in any::<usize>()) {
            let serialized = to_vec(x);
            let (deserialized, _) = read(Cursor::new(serialized)).unwrap();
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
        let err = read(Cursor::new(serialized)).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }
}
