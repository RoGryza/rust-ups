use std::fmt::{self, Debug, Display, Formatter, LowerHex, UpperHex};

use crc32fast::Hasher;

/// A CRC-32 checksum.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Checksum(pub u32);

impl Checksum {
    /// Calculate `data` checksum.
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(&data);
        Checksum(hasher.finalize())
    }
}

impl Debug for Checksum {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "Checksum({:x})", self)
    }
}

impl Display for Checksum {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        UpperHex::fmt(self, f)
    }
}

impl LowerHex for Checksum {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("0x")?;
        for byte in &self.0.to_le_bytes() {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

impl UpperHex for Checksum {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("0x")?;
        for byte in &self.0.to_le_bytes() {
            write!(f, "{:02X}", byte)?;
        }
        Ok(())
    }
}
