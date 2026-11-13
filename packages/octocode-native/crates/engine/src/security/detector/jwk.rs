//! A JSON Web Key's private members in a cut of a source too large to scan whole.
//! Only `kty` says what `d`, `p` or `k` are; a walk of the source's objects
//! ([`super::objects`]) pairs them where it can see both.
use regex::Regex;
use std::sync::LazyLock;

/// One private member, in any spelling a walk of its object reads.
#[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
static MEMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?m)(?P<name>(?:^|[ \t\r\n,{"'])(?:d|p|q|dp|dq|qi|k|priv)\\?["']?[ \t\r\n]*:[ \t\r\n]*\\?["']?)(?P<value>[A-Za-z0-9_+/-]{16,}={0,2})(?P<end>\\?["']|[ \t\r,}\n]|$)"#,
    )
    .expect("static pattern compiles")
});

/// The member that says its object is a JSON Web Key.
pub(super) const KEY_TYPE: &str = "kty";

/// The pattern that holds these members.
pub(super) const PATTERN: &str = "jwkPrivateMembers";

/// Whether `text` shows a member only its object can judge.
pub(super) fn shows_member(text: &str) -> bool {
    MEMBER.is_match(text)
}
