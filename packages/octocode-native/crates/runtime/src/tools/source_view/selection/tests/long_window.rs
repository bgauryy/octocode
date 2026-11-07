//! A read whose evaluations the default window refused and the long-context
//! window answered says so on its first page, with the tokens billed.
use super::*;
use crate::tools::selection::long_window_fake::{BILLED, LongWindow};

#[test]
fn a_long_context_retry_is_disclosed_on_the_classified_read() {
    use crate::execution::cancel::NeverCancel;
    let window = LongWindow::start();
    let query: LocalFetchQuery = serde_json::from_value(json!({
        "path":"source.txt",
        "classification":{"predicates":["Is it kept?"]}
    }))
    .unwrap();
    let result = crate::tools::source_view::process_fetched_content_selected(
        &query,
        b"keep one\nother\nkeep two\n",
        Path::new("source.txt"),
        "source.txt",
        &crate::tools::source_view::SourceFacts::default(),
        &ContentSecurity::new(),
        &NeverCancel,
        Some(&window.selection),
    );
    let retried = u64::try_from(window.long_requests()).unwrap();
    assert!(retried > 0, "the provider answered in the long window");
    let notice = result
        .warnings
        .iter()
        .find(|note| note.contains("long-context window"))
        .unwrap_or_else(|| panic!("no retry notice: {:?}", result.warnings));
    let evaluations = octocode_engine::text::counted(retried, "evaluation", "evaluations");
    assert!(notice.starts_with(&evaluations), "{notice}");
    assert!(
        notice.contains(&format!("{} input tokens billed", retried * BILLED)),
        "{notice}"
    );
    assert!(
        notice.contains("longContext:false refuses instead"),
        "{notice}"
    );
}
