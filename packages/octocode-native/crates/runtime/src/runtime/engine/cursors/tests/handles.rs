use super::*;

/// The same continuation (tool, canonical input, saved work) issued to the
/// same access identity has one handle, whichever output issues it and in
/// whatever key order its input is spelled; anything else that differs
/// issues another. Retaining it again keeps it reachable.
#[test]
fn an_identical_continuation_has_one_handle() {
    let registry = registry("policy");
    let issue = |access: u8, tool, input: Value, index| {
        let mut pending = Pending::new([access; 32], None);
        let token = pending.insert(tool, input, SavedWork::ToolResume { index: Some(index) });
        assert!(registry.retain(pending, &context()).is_ok());
        token
    };
    let lead = || json!({"queries":[{"path":"src","importerPage":2,"snapshot":"s"}]});
    let first = issue(1, ToolId::LspSearch, lead(), 0);
    let reordered = json!({"queries":[{"snapshot":"s","importerPage":2,"path":"src"}]});
    assert_eq!(issue(1, ToolId::LspSearch, reordered, 0), first);
    assert_eq!(issue(1, ToolId::LspSearch, lead(), 0), first);
    for other in [
        issue(2, ToolId::LspSearch, lead(), 0),
        issue(1, ToolId::LocalSearch, lead(), 0),
        issue(1, ToolId::LspSearch, lead(), 1),
        issue(
            1,
            ToolId::LspSearch,
            json!({"queries":[{"path":"src","importerPage":3,"snapshot":"s"}]}),
            0,
        ),
    ] {
        assert_ne!(other, first);
    }
    let opened = registry
        .open(ToolId::LspSearch, &first, [1; 32])
        .expect("retained");
    assert_eq!(opened.member().input, lead());
    assert!(registry.open(ToolId::LspSearch, &first, [2; 32]).is_err());
    assert_eq!(
        first.len(),
        crate::response::cursor::issued::placeholder().len()
    );
}
