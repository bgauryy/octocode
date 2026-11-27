use super::*;
use std::time::Duration;

struct Until(Instant);
impl CancellationCheck for Until {
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
    fn deadline(&self) -> Option<Instant> {
        Some(self.0)
    }
}

/// A resumable build extracts for half of what its call has left; one
/// that cannot resume, or has no deadline, until the call stops.
#[test]
fn extraction_takes_half_of_a_resumable_calls_budget() {
    let budget = Duration::from_secs(600);
    let request = Until(Instant::now() + budget);
    let turn = ExtractionTurn::new(&request, true);
    let ends = turn.deadline().expect("a share");
    let share = ends.saturating_duration_since(Instant::now());
    assert!(
        share <= budget / 2 && share + Duration::from_secs(60) > budget / 2,
        "{share:?}"
    );
    assert!(turn.check().is_ok() && !turn.ended());
    assert_eq!(
        ExtractionTurn::new(&request, false).deadline(),
        Some(request.0)
    );
    let never = octocode_runtime_base::execution::cancel::NeverCancel;
    assert_eq!(ExtractionTurn::new(&never, true).deadline(), None);
    let spent = Until(Instant::now());
    let turn = ExtractionTurn::new(&spent, true);
    assert!(turn.check().is_err() && turn.ended());
}
