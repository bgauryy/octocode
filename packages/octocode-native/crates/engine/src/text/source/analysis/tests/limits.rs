//! Every limit a [`MapBudget`] can refuse with names itself and its
//! numbers in the refusal ([`MapBudget::exceeded`]).
use super::*;

#[test]
fn every_limit_name_is_worded_with_its_numbers() {
    let mut budget = MapBudget::new(
        MapLimits {
            bytes_budget: "build memory budget",
            ..MapLimits::default()
        },
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        &|| false,
    );
    budget.allocate(100).unwrap();
    budget.usage.segments = 7;
    let limits = MapLimits::default();
    let cases = [
        (
            "bytes",
            format!(
                "needs more than the build memory budget of {} bytes (100 charged when it stopped)",
                limits.bytes
            ),
        ),
        (
            "segments",
            format!(
                "passes its segments limit of {} (7 used when it stopped)",
                limits.segments
            ),
        ),
        ("depth", format!("passes its depth limit of {}", limits.depth)),
        (
            "alternatives",
            format!(
                "passes its alternatives limit of {} (0 used when it stopped)",
                limits.branches
            ),
        ),
        (
            "allocation",
            "needs an allocation this platform cannot size or make (allocation; 100 bytes charged when it stopped)".to_owned(),
        ),
        (
            "boundaries",
            "needs an allocation this platform cannot size or make (boundaries; 100 bytes charged when it stopped)".to_owned(),
        ),
    ];
    for (name, expected) in cases {
        assert_eq!(budget.exceeded(name), expected, "{name}");
    }
    for name in ["work", "representations", "branches", "contributors"] {
        let worded = budget.exceeded(name);
        assert!(
            worded.starts_with(&format!("passes its {name} limit of "))
                && worded.contains(" used when it stopped)"),
            "{name}: {worded}"
        );
    }
}
