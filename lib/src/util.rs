use std::ops::Range;

pub struct SliceDiffs<'a> {
    index: usize,
    a: &'a [u8],
    b: &'a [u8],
}

impl<'a> SliceDiffs<'a> {
    pub fn new(a: &'a [u8], b: &'a [u8]) -> Self {
        SliceDiffs { index: 0, a, b }
    }
}

impl<'a> Iterator for SliceDiffs<'a> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        let rel_start = self.a.iter().zip(self.b).position(|(a, b)| a != b)?;
        let a = &self.a[rel_start..];
        let b = &self.b[rel_start..];
        let rel_end = a
            .iter()
            .zip(b)
            .position(|(a, b)| a == b)
            .unwrap_or(std::cmp::min(a.len(), b.len()));
        self.a = &a[rel_end..];
        self.b = &b[rel_end..];
        let start = rel_start + self.index;
        let end = start + rel_end;
        self.index += rel_start + rel_end;
        Some(start..end)
    }
}
