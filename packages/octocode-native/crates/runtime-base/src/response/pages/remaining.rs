pub use octocode_engine::text::counted;

/// The first `limit` of `names` joined with `, `, then the count of the
/// rest: `a, b, +3 more`.
pub fn first_named(names: impl IntoIterator<Item = String>, limit: usize) -> String {
    let mut names = names.into_iter();
    let mut shown = names.by_ref().take(limit).collect::<Vec<_>>();
    let rest = names.count();
    if rest > 0 {
        shown.push(format!("+{rest} more"));
    }
    shown.join(", ")
}
