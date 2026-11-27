//! ghSearchHistory and ghGetHistoryItem (commits, issues, pull requests)
//! through the runtime against a mock GitHub.

mod commits;
mod continuation_semantics;
mod failures;
mod history_a;
mod history_a_commits;
mod issue_comments;
mod issue_reads;
mod patch_page_redaction;
mod pr_repeat_window;
mod pull_requests;
mod search;

const HISTORY_A_HEAD: &str = "0123456789abcdef0123456789abcdef01234567";
