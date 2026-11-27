//! A source table's passes end with the request that runs them, not at a
//! fixed timeout of their own.
use super::*;
use std::time::{Duration, Instant};

/// Work whose request ran out of time without being cancelled.
struct Expired;
impl CancellationCheck for Expired {
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
    fn deadline(&self) -> Option<Instant> {
        Instant::now().checked_sub(Duration::from_millis(1))
    }
}

#[test]
fn a_pass_ends_at_its_requests_deadline() {
    assert!(matches!(
        budget::Budget::new(&Expired, &LIMITS).check(),
        Err(SourceError::Deadline)
    ));
    assert!(budget::Budget::new(&NeverCancel, &LIMITS).check().is_ok());
}
