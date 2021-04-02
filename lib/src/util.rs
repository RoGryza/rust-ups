use std::ops::Range;

#[cfg(test)]
pub use self::test::*;

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

#[cfg(test)]
mod test {
    use proptest::test_runner::{Reason, TestCaseError};
    use std::fmt::Debug;

    /// Unwrap implementations that play nicer with proptest.
    pub trait ProptestUnwrapExt: Sized {
        type Ok;
        type Error;

        fn default_msg(err: Self::Error) -> String;
        fn default_err_msg(ok: Self::Ok) -> String;
        // Since std::ops::Try is still unstable.
        fn into_result(self) -> Result<Self::Ok, Self::Error>;

        fn prop_expect(self, msg: impl Into<Reason>) -> Result<Self::Ok, TestCaseError> {
            self.into_result().map_err(|_| TestCaseError::fail(msg))
        }

        fn prop_expect_err(self, msg: impl Into<Reason>) -> Result<Self::Error, TestCaseError> {
            match self.into_result() {
                Ok(_) => Err(TestCaseError::fail(msg)),
                Err(e) => Ok(e),
            }
        }

        fn prop_unwrap(self) -> Result<Self::Ok, TestCaseError> {
            self.into_result()
                .map_err(|e| TestCaseError::fail(Self::default_msg(e)))
        }

        fn prop_unwrap_err(self) -> Result<Self::Error, TestCaseError> {
            match self.into_result() {
                Ok(x) => Err(TestCaseError::fail(Self::default_err_msg(x))),
                Err(e) => Ok(e),
            }
        }
    }

    impl<T: Debug> ProptestUnwrapExt for Option<T> {
        type Ok = T;
        type Error = ();

        fn default_msg(_: ()) -> String {
            "Called `Option::prop_unwrap` on a `None` value".into()
        }

        fn default_err_msg(ok: T) -> String {
            format!(
                "Called `Option::prop_unwrap_err` on a `Some` value: {:?}",
                ok
            )
        }

        fn into_result(self) -> Result<T, ()> {
            self.ok_or(())
        }
    }

    impl<T: Debug, E: Debug> ProptestUnwrapExt for Result<T, E> {
        type Ok = T;
        type Error = E;

        fn default_msg(err: E) -> String {
            format!("Called `Result::prop_unwrap` on an `Err` value: {:?}", err)
        }

        fn default_err_msg(ok: T) -> String {
            format!(
                "Called `Result::prop_unwrap_err` on an `Ok` value: {:?}",
                ok
            )
        }

        fn into_result(self) -> Result<T, E> {
            self
        }
    }
}
