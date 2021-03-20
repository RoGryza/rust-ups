use std::fmt::{self, Debug, Display, Formatter, LowerHex, UpperHex};
use std::io::{self, Read, Write};

use crc32fast::Hasher;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Checksum(pub u32);

#[derive(Debug, Clone)]
pub struct ChecksumStream<S> {
    inner: S,
    hasher: Hasher,
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

impl<S> ChecksumStream<S> {
    pub fn new(stream: S) -> Self {
        ChecksumStream {
            inner: stream,
            hasher: Hasher::new(),
        }
    }

    pub fn finalize(self) -> (S, Checksum) {
        (self.inner, Checksum(self.hasher.finalize()))
    }
}

impl<S: Read> ChecksumStream<S> {
    pub fn calculate_checksum(mut self) -> io::Result<Checksum> {
        let mut buf = [0u8; 4096];
        loop {
            if self.read(&mut buf)? == 0 {
                return Ok(Checksum(self.hasher.finalize()));
            }
        }
    }
}

impl<S: Read> Read for ChecksumStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let res = self.inner.read(buf);
        if let Ok(n) = res {
            self.hasher.update(&buf[..n]);
        }
        res
    }
}

impl<S: Write> Write for ChecksumStream<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let res = self.inner.write(buf);
        if let Ok(n) = res {
            self.hasher.update(&buf[..n]);
        }
        res
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
