use super::*;

/// k computing calls each take `cores / k`, so together they never take
/// more than the cores (or one thread each, when there are more calls
/// than cores); a call that ends returns its share.
#[test]
fn computing_calls_split_the_cores_and_never_exceed_them() {
    let budget = CpuBudget::new(12);
    assert_eq!(budget.share(), 12, "no call: a fan-out takes every core");
    for calls in 1..=30 {
        let guards = (0..calls).map(|_| budget.enter()).collect::<Vec<_>>();
        let share = budget.share();
        assert!(share >= 1);
        assert!(
            share * calls <= budget.cores().max(calls),
            "{calls} calls of {share} threads"
        );
        drop(guards);
        assert_eq!(budget.share(), 12, "every share returned");
    }
    assert_eq!(
        CpuBudget::new(0).share(),
        1,
        "an unknown host runs one thread"
    );
}
