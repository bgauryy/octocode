//! Which paths a filesystem walk leaves out: one value every walk request
//! (text search, file query, structural search, graph scan) carries.

/// Which paths a walk leaves out, derived once from a request's
/// `includeIgnored` and `includeHidden` flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IgnorePolicy {
    /// `includeIgnored`: walk what ignore files exclude. Lifts every source:
    /// `.gitignore`, `.ignore`, the global gitignore, `.git/info/exclude`,
    /// and the ignore files above the root.
    pub include_ignored: bool,
    /// `includeHidden`: walk dot files and dot directories.
    pub include_hidden: bool,
}

impl IgnorePolicy {
    /// The policy of a request's optional flags (both default off).
    pub fn new(include_ignored: Option<bool>, include_hidden: Option<bool>) -> Self {
        Self {
            include_ignored: include_ignored == Some(true),
            include_hidden: include_hidden == Some(true),
        }
    }

    /// Everything walked: no ignore rule and no dot entry left out.
    pub const ALL: Self = Self {
        include_ignored: true,
        include_hidden: true,
    };
}
