use super::*;

#[test]
fn pre_cancelled_requests_stop_every_transient_operation() {
    let catalog = indexed(100, true);
    let fresh = CancellationToken::new();
    let set = catalog.dependencies(&origin(0, true), &fresh).unwrap();
    let cancelled = CancellationToken::new();
    cancelled.cancel();

    let mut original_row = json!({"sourceEvidence":descriptors(2, true)});
    let before = original_row.clone();
    assert!(matches!(
        Catalog::take(&mut original_row, &cancelled),
        Err(DependencyError::Cancelled)
    ));
    assert_eq!(original_row, before);
    assert!(matches!(
        catalog.dependencies(&origin(0, true), &cancelled),
        Err(DependencyError::Cancelled)
    ));
    assert_eq!(
        catalog.selected(&set, &cancelled),
        Err(DependencyError::Cancelled)
    );
    assert_eq!(
        catalog.delta(&catalog.empty(), &set, &cancelled),
        Err(DependencyError::Cancelled)
    );
    let mut target = catalog.empty();
    assert_eq!(
        catalog.merge_delta(&mut target, &set, &cancelled),
        Err(DependencyError::Cancelled)
    );
    assert!(target.is_empty());
    let mut row = json!({"data":{"items":[]}});
    let before = row.clone();
    assert_eq!(
        catalog.attach(&mut row, &set, &cancelled),
        Err(DependencyError::Cancelled)
    );
    assert_eq!(row, before);
}

#[test]
fn cancellation_does_not_poison_catalogs_or_later_replay() {
    let first = CancellationToken::new();
    let mut row = json!({"data":{"sourceEvidence":descriptors(100, true)}});
    let catalog = Catalog::take(&mut row, &first).unwrap().unwrap();
    let old_set = catalog.dependencies(&origin(0, true), &first).unwrap();
    let expected = catalog.selected(&old_set, &first).unwrap();
    first.cancel();
    assert!(matches!(
        catalog.dependencies(&origin(0, true), &first),
        Err(DependencyError::Cancelled)
    ));

    let replay = CancellationToken::new();
    let new_set = catalog.dependencies(&origin(0, true), &replay).unwrap();
    assert_eq!(catalog.selected(&old_set, &replay).unwrap(), expected);
    assert_eq!(catalog.selected(&new_set, &replay).unwrap(), expected);
    let mut joined = old_set.clone();
    catalog.merge_delta(&mut joined, &new_set, &replay).unwrap();
    assert_eq!(
        catalog.chars(&joined).unwrap(),
        catalog.chars(&old_set).unwrap()
    );
    catalog.attach(&mut row, &joined, &replay).unwrap();
    assert_eq!(row["data"]["sourceEvidence"], expected);
}

#[test]
fn ordinary_foreign_owner_error_keeps_accumulator_unchanged() {
    let catalog = indexed(100, true);
    let other = indexed(100, true);
    let token = CancellationToken::new();
    let mut set = catalog.dependencies(&origin(0, true), &token).unwrap();
    let before = catalog.selected(&set, &token).unwrap();
    let chars = catalog.chars(&set).unwrap();
    let foreign = other.dependencies(&origin(1, true), &token).unwrap();
    assert_eq!(
        catalog.merge_delta(&mut set, &foreign, &token),
        Err(DependencyError::ForeignSet)
    );
    assert_eq!(catalog.chars(&set).unwrap(), chars);
    assert_eq!(catalog.selected(&set, &token).unwrap(), before);
}
