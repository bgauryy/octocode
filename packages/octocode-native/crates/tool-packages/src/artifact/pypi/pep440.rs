//! PEP 440 versions: parsing with the spec's normalisation (case, `v`
//! prefix, separators, alternate spellings, implicit numbers) and its
//! total order (epoch, release, pre, post, dev, local).
use std::cmp::Ordering;

mod specifier;
#[cfg(test)]
mod tests;

pub(super) use specifier::Specifiers;

/// A pre-release phase, in release order (`a` < `b` < `rc`).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Phase {
    Alpha,
    Beta,
    Candidate,
}

/// A local-label segment: any number sorts after any text.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Label {
    Text(String),
    Number(u64),
}

/// A parsed PEP 440 version. Equality is the spec's: `1.0 == 1.0.0`.
#[derive(Clone, Debug)]
pub(super) struct Version {
    epoch: u64,
    release: Vec<u64>,
    pre: Option<(Phase, u64)>,
    post: Option<u64>,
    dev: Option<u64>,
    local: Option<Vec<Label>>,
}

impl Version {
    /// The version `text` spells, or `None` when it is not PEP 440.
    pub(super) fn parse(text: &str) -> Option<Self> {
        let lower = text.trim().to_ascii_lowercase();
        let mut cursor = Cursor(lower.strip_prefix('v').unwrap_or(&lower));
        let mut epoch = 0;
        let mut ahead = cursor;
        if let Some(digits) = ahead.number()
            && ahead.eat("!")
        {
            epoch = digits.parse().ok()?;
            cursor = ahead;
        }
        let mut release = vec![cursor.number()?.parse().ok()?];
        loop {
            let mut ahead = cursor;
            match ahead.eat(".").then(|| ahead.number()).flatten() {
                Some(digits) => release.push(digits.parse().ok()?),
                None => break,
            }
            cursor = ahead;
        }
        let pre = cursor.phase();
        let post = cursor.post();
        let dev = cursor.labelled(&["dev"]).map(|(_, number)| number);
        let local = if cursor.eat("+") {
            Some(cursor.local()?)
        } else {
            None
        };
        if !cursor.0.is_empty() {
            return None;
        }
        Some(Self {
            epoch,
            release,
            pre: pre.transpose().ok()?,
            post: post.transpose().ok()?,
            dev: dev.transpose().ok()?,
            local,
        })
    }

    /// A pre-release or a development release.
    pub(super) fn is_prerelease(&self) -> bool {
        self.pre.is_some() || self.dev.is_some()
    }

    pub(super) fn has_local(&self) -> bool {
        self.local.is_some()
    }

    pub(super) fn release_len(&self) -> usize {
        self.release.len()
    }

    /// Only an epoch and release numbers: what a `.*` prefix may name.
    pub(super) fn is_bare_release(&self) -> bool {
        self.pre.is_none() && self.post.is_none() && self.dev.is_none() && self.local.is_none()
    }

    /// The order ignoring local labels (`1.0+abc` and `1.0` compare equal).
    pub(super) fn cmp_public(&self, other: &Self) -> Ordering {
        self.key(false).cmp(&other.key(false))
    }

    /// For a version above `base`: whether it is a local build or a post
    /// release of `base` (`1.0+abc`, `1.0.post2`), which `>1.0` excludes.
    pub(super) fn is_post_of(&self, base: &Self) -> bool {
        self.epoch == base.epoch
            && trimmed(&self.release) == trimmed(&base.release)
            && self.pre == base.pre
    }

    /// This version with only `dev` and no local label: the bounds `<`,
    /// `>` and the wildcard operators compare against.
    pub(super) fn with_dev(&self, dev: u64) -> Self {
        Self {
            dev: Some(dev),
            local: None,
            ..self.clone()
        }
    }

    /// The first development release of post release `post` of this
    /// version (`1.0.post3.dev0`), without a local label.
    pub(super) fn post_dev0(&self, post: u64) -> Self {
        Self {
            post: Some(post),
            dev: Some(0),
            local: None,
            ..self.clone()
        }
    }

    pub(super) fn dev(&self) -> Option<u64> {
        self.dev
    }

    pub(super) fn post(&self) -> Option<u64> {
        self.post
    }

    /// The first `.dev0` of release prefix `prefix` and of the prefix after
    /// it: `1.2` gives `[1.2.dev0, 1.3.dev0)`, the versions `==1.2.*` names.
    pub(super) fn prefix_bounds(&self, prefix: usize) -> (Self, Self) {
        let bare = |release: Vec<u64>| Self {
            epoch: self.epoch,
            release,
            pre: None,
            post: None,
            dev: Some(0),
            local: None,
        };
        let lower = self.release[..prefix].to_vec();
        let mut upper = lower.clone();
        if let Some(last) = upper.last_mut() {
            *last = last.saturating_add(1);
        }
        (bare(lower), bare(upper))
    }

    /// The sort key: a dev release of a final sorts before its
    /// pre-releases, a final after them; no post sorts first, no dev last,
    /// no local first.
    fn key(&self, local: bool) -> impl Ord + '_ {
        let pre = match (self.pre, self.post, self.dev) {
            (None, None, Some(_)) => (0, None),
            (Some(pre), _, _) => (1, Some(pre)),
            (None, _, _) => (2, None),
        };
        (
            self.epoch,
            trimmed(&self.release),
            pre,
            self.post,
            (self.dev.is_none(), self.dev),
            self.local.as_deref().filter(|_| local),
        )
    }
}

fn trimmed(release: &[u64]) -> &[u64] {
    let end = release
        .iter()
        .rposition(|part| *part != 0)
        .map_or(0, |at| at + 1);
    &release[..end]
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key(true).cmp(&other.key(true))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for Version {}

/// The unparsed rest of a lowercased version.
#[derive(Clone, Copy)]
struct Cursor<'a>(&'a str);

impl<'a> Cursor<'a> {
    fn eat(&mut self, prefix: &str) -> bool {
        self.0
            .strip_prefix(prefix)
            .map(|rest| self.0 = rest)
            .is_some()
    }

    /// One optional `-`, `_` or `.` separator.
    fn separator(&mut self) {
        let _ = self.eat("-") || self.eat("_") || self.eat(".");
    }

    fn number(&mut self) -> Option<&'a str> {
        let end = self
            .0
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(self.0.len());
        let (digits, rest) = self.0.split_at(end);
        (!digits.is_empty()).then(|| {
            self.0 = rest;
            digits
        })
    }

    /// `[sep] label [sep] [number]`, the number 0 when omitted (`1.0a.`
    /// is `1.0a0`); the cursor stays put when no label follows. `Err` on
    /// an overflow.
    fn labelled(&mut self, labels: &[&'static str]) -> Option<(&'static str, Result<u64, ()>)> {
        let mut ahead = *self;
        ahead.separator();
        let label = labels.iter().find(|label| ahead.eat(label))?;
        ahead.separator();
        let value = ahead
            .number()
            .map_or(Ok(0), |digits| digits.parse().map_err(drop));
        *self = ahead;
        Some((label, value))
    }

    fn phase(&mut self) -> Option<Result<(Phase, u64), ()>> {
        const LABELS: [&str; 8] = ["alpha", "a", "beta", "b", "preview", "pre", "c", "rc"];
        let (label, number) = self.labelled(&LABELS)?;
        let phase = match label {
            "alpha" | "a" => Phase::Alpha,
            "beta" | "b" => Phase::Beta,
            _ => Phase::Candidate,
        };
        Some(number.map(|number| (phase, number)))
    }

    /// `-N`, or `[sep] post|rev|r [sep] [N]`.
    fn post(&mut self) -> Option<Result<u64, ()>> {
        let mut ahead = *self;
        if ahead.eat("-")
            && let Some(digits) = ahead.number()
        {
            *self = ahead;
            return Some(digits.parse().map_err(drop));
        }
        self.labelled(&["post", "rev", "r"])
            .map(|(_, number)| number)
    }

    /// `[a-z0-9]+` segments joined by `-`, `_` or `.`.
    fn local(&mut self) -> Option<Vec<Label>> {
        let segments = self.0.split(['-', '_', '.']).map(|segment| {
            if segment.is_empty() || !segment.bytes().all(|b| b.is_ascii_alphanumeric()) {
                None
            } else if segment.bytes().all(|b| b.is_ascii_digit()) {
                segment.parse().ok().map(Label::Number)
            } else {
                Some(Label::Text(segment.to_owned()))
            }
        });
        let labels = segments.collect::<Option<Vec<_>>>()?;
        self.0 = "";
        Some(labels)
    }
}
