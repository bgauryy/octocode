//! What judging a row's units shares: how units are packed into provider
//! requests and answered ([`judge`]), and how a tool's rows become units
//! ([`row_units`]).
use serde_json::Value;

pub mod judge;
#[cfg(any(test, feature = "test-support"))]
pub mod long_window_fake;
#[cfg(any(test, feature = "test-support"))]
pub mod transient_fake;
pub use judge::section_question;
pub mod row_units;
pub use row_units::entry_name;

/// Unicode scalars of `value` serialized as JSON, counted while writing
/// instead of building the string.
pub fn serialized_chars(value: &Value) -> usize {
    struct Chars(usize);
    impl std::io::Write for Chars {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            // Every scalar has exactly one non-continuation byte.
            self.0 += bytes.iter().filter(|&&byte| byte & 0xC0 != 0x80).count();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut chars = Chars(0);
    serde_json::to_writer(&mut chars, value).map_or(0, |()| chars.0)
}
