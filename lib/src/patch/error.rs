use std::error::Error;
use std::fmt::{self, Debug, Display, Formatter};
use std::iter::FusedIterator;

use crate::{Checksum, Patch};

/// Possible errors when parsing an UPS patch file.
#[derive(thiserror::Error, Debug)]
pub enum UpsParseError {
    #[error("this doesn't seem to be an UPS file: {}", .0)]
    FormatMismatch(String),
    /// Calculated patch checksum doesn't match the one from the patch metadata. You can access the
    /// patch in `parsed_patch` in case you want to ignore checksum errors.
    #[error(
        "checksum mismatch for patch file: expected {}, got {}",
        .expected, .actual,
    )]
    PatchChecksumMismatch {
        parsed_patch: Patch,
        expected: Checksum,
        actual: Checksum,
    },
}

pub type UpsParseResult<T> = Result<T, UpsParseError>;

/// Collection of errors returned from patching. You can access the patched file in `output` in
/// case you want to ignore the errors. Use [`iter`](UpsPatchErrors::iter) and
/// [`into_iter`](IntoIterator::into_iter) to inspect errors.
pub struct UpsPatchErrors {
    /// Possibly invalid output from the patch operation.
    pub output: Vec<u8>,
    // Standalone error to enforce that the error list is non-empty.
    fst_error: UpsPatchError,
    errors: Vec<UpsPatchError>,
}

impl UpsPatchErrors {
    /// Smart constructor, returns `Err` if `errors` is not empty, else returns `Ok(output)`.
    pub fn check_errors(output: Vec<u8>, mut errors: Vec<UpsPatchError>) -> Result<Vec<u8>, Self> {
        // There's no order for errors so we just pop fst_error from errors.
        match errors.pop() {
            Some(fst_error) => Err(UpsPatchErrors {
                output,
                fst_error,
                errors,
            }),
            None => Ok(output),
        }
    }

    /// Iterate over all patching errors by reference.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &UpsPatchError> {
        self.into_iter()
    }
}

#[derive(Debug, Clone)]
pub struct ErrorsIntoIter(
    std::iter::Chain<std::iter::Once<UpsPatchError>, std::vec::IntoIter<UpsPatchError>>,
);

impl IntoIterator for UpsPatchErrors {
    type Item = UpsPatchError;
    type IntoIter = ErrorsIntoIter;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        ErrorsIntoIter(std::iter::once(self.fst_error).chain(self.errors))
    }
}

impl Iterator for ErrorsIntoIter {
    type Item = UpsPatchError;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    #[inline]
    fn count(self) -> usize {
        self.0.count()
    }
}

impl FusedIterator for ErrorsIntoIter {}

impl DoubleEndedIterator for ErrorsIntoIter {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }
}

#[derive(Debug, Clone)]
pub struct ErrorsIter<'a>(
    std::iter::Chain<std::iter::Once<&'a UpsPatchError>, std::slice::Iter<'a, UpsPatchError>>,
);

impl<'a> IntoIterator for &'a UpsPatchErrors {
    type Item = &'a UpsPatchError;
    type IntoIter = ErrorsIter<'a>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        ErrorsIter(std::iter::once(&self.fst_error).chain(&self.errors))
    }
}

impl<'a> Iterator for ErrorsIter<'a> {
    type Item = &'a UpsPatchError;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    #[inline]
    fn count(self) -> usize {
        self.0.count()
    }
}

impl<'a> FusedIterator for ErrorsIter<'a> {}

impl<'a> DoubleEndedIterator for ErrorsIter<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }
}

impl Debug for UpsPatchErrors {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut dbg_errors = vec![&self.fst_error];
        dbg_errors.extend(&self.errors);
        f.debug_struct("UpsPatchErrors")
            .field("errors", &dbg_errors)
            .finish()
    }
}

impl Display for UpsPatchErrors {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if self.errors.is_empty() {
            write!(f, "{}", self.fst_error)?;
        } else {
            write!(f, "multiple errors: {}", self.fst_error)?;
            for err in &self.errors {
                write!(f, ", {}", err)?;
            }
        }
        Ok(())
    }
}

impl Error for UpsPatchErrors {
    // TODO multiple sources?
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.fst_error)
    }
}

/// Possible errors when applying or reverting an UPS patch.
#[derive(thiserror::Error, Debug, Clone)]
pub enum UpsPatchError {
    #[error("source file {}", .0)]
    SourceMetadataMismatch(MetadataMismatch),
    #[error("destination file {}", .0)]
    DestMetadataMismatch(MetadataMismatch),
}

pub type UpsPatchResult<T> = Result<T, UpsPatchErrors>;

/// Kinds of metadata mismatches for [`UpsPatchError`].
#[derive(Debug, Clone)]
pub enum MetadataMismatch {
    Size {
        expected: usize,
        actual: usize,
    },
    Checksum {
        expected: Checksum,
        actual: Checksum,
    },
}

impl MetadataMismatch {
    pub fn size(expected: usize, actual: usize) -> Option<Self> {
        if expected == actual {
            None
        } else {
            Some(MetadataMismatch::Size { expected, actual })
        }
    }

    pub fn checksum(expected: Checksum, actual: Checksum) -> Option<Self> {
        if expected == actual {
            None
        } else {
            Some(MetadataMismatch::Checksum { expected, actual })
        }
    }
}

impl Display for MetadataMismatch {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            MetadataMismatch::Size { expected, actual } => {
                write!(f, "size mismatch: expected {}, got {}", expected, actual)
            }
            MetadataMismatch::Checksum { expected, actual } => write!(
                f,
                "checksum mismatch: expected {}, got {}",
                expected, actual,
            ),
        }
    }
}
