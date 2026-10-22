//! Internal limits: code constants, never user configuration. `DEFAULT` is
//! the one place each value lives; `ToolRuntime::with_limits` is the test
//! seam for a non-default value. Nothing here reads the environment or a
//! file.
use std::time::Duration;

/// Every internal limit a runtime's tools run under.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeLimits {
    /// Request time of one network call (GitHub, registries, the
    /// classification provider), within the call's own deadline.
    pub network_timeout: Duration,
    /// Retries after a failed request.
    pub max_retries: u8,
    /// Longest GitHub rate-limit wait (primary or secondary limit,
    /// code-search window) before a call fails with `retryAfter`. It stays
    /// under the 60 s tool-call timeout some MCP hosts apply.
    pub max_rate_limit_wait: Duration,
    /// Characters of the automatic response page when a call names no
    /// `responseLength`.
    pub auto_page_chars: usize,
    /// The ghCloneRepo cache.
    pub clone: CloneLimits,
    /// Process-wide in-flight classification provider requests per endpoint.
    pub classification_concurrency: usize,
    /// One context selection operation.
    pub selection: SelectionLimits,
    /// Client-side GitHub request spacing: octokit's in production; tests
    /// that do not exercise throttling run unpaced.
    pub github_pacing: octocode_github::GitHubPacing,
}

/// The on-disk ghCloneRepo cache.
#[derive(Clone, Copy, Debug)]
pub struct CloneLimits {
    /// How long a checkout stays fresh before it is re-fetched; also how
    /// long a clasify URL capture is kept.
    pub ttl: Duration,
    /// Byte cap; least-recently-used checkouts are evicted above it.
    pub(crate) max_bytes: u64,
    /// Repositories the cache keeps.
    pub(crate) max_clones: usize,
}

/// One context selection operation.
#[derive(Clone, Copy, Debug)]
pub struct SelectionLimits {
    /// Complete evidence units assessed; unassessed units stay unknown and
    /// recoverable.
    pub max_candidates: usize,
    /// Total characters of serialized sanitized candidate states, including
    /// source metadata.
    pub max_chars: usize,
    /// Deadline, bounded by the enclosing request deadline.
    pub timeout: Duration,
}

impl RuntimeLimits {
    pub const DEFAULT: Self = Self {
        network_timeout: Duration::from_secs(30),
        max_retries: 3,
        max_rate_limit_wait: Duration::from_secs(45),
        auto_page_chars: 20_000,
        clone: CloneLimits {
            ttl: Duration::from_secs(24 * 60 * 60),
            max_bytes: 2 * 1024 * 1024 * 1024,
            max_clones: 50,
        },
        classification_concurrency: 10,
        selection: SelectionLimits {
            max_candidates: 64,
            max_chars: 131_072,
            timeout: Duration::from_secs(10),
        },
        github_pacing: octocode_github::GitHubPacing::Octokit,
    };
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeLimits;
    use std::time::Duration;

    /// `131072` as `131,072`, the way the reasons write counts.
    fn grouped(count: usize) -> String {
        let digits = count.to_string();
        let mut out = String::new();
        for (at, digit) in digits.chars().enumerate() {
            if at > 0 && (digits.len() - at).is_multiple_of(3) {
                out.push(',');
            }
            out.push(digit);
        }
        out
    }

    /// Whether `reason` states `value` as a whole phrase: `fixed at 3` is
    /// not stated by `fixed at 30`, `fixed at 3.5`, `fixed at 3,000` or
    /// `unfixed at 3`.
    fn states(reason: &str, value: &str) -> bool {
        reason.match_indices(value).any(|(at, _)| {
            let starts = !reason[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric);
            let mut rest = reason[at + value.len()..].chars();
            let ends = match rest.next() {
                None => true,
                Some('.' | ',') => !rest.next().is_some_and(|next| next.is_ascii_digit()),
                Some(next) => !next.is_alphanumeric(),
            };
            starts && ends
        })
    }

    #[test]
    fn a_stated_value_is_a_whole_phrase() {
        for reason in [
            "Retries are fixed at 3; delete the key.",
            "fixed at 3",
            "fixed at 3.",
        ] {
            assert!(states(reason, "fixed at 3"), "{reason}");
        }
        for reason in [
            "Retries are fixed at 30; delete the key.",
            "fixed at 3.5 s",
            "fixed at 3,000 characters",
            "unfixed at 3.",
        ] {
            assert!(!states(reason, "fixed at 3"), "{reason}");
        }
    }

    /// The config contract's reason for each removed limit setting and
    /// variable states the value it is fixed at, written from `DEFAULT`:
    /// changing a limit fails here until the contract's text says the new
    /// value.
    #[test]
    fn removed_limit_settings_state_the_default_values() {
        let limits = RuntimeLimits::DEFAULT;
        let seconds = |duration: Duration| format!("{} s", duration.as_secs());
        let clone = limits.clone;
        let selection = limits.selection;
        let cases: [(&[&str], String); 11] = [
            (
                &["network.timeout", "REQUEST_TIMEOUT"],
                format!("fixed at {}", seconds(limits.network_timeout)),
            ),
            (
                &["network.maxRetries", "MAX_RETRIES"],
                format!("fixed at {}", limits.max_retries),
            ),
            (
                &["network.maxRateLimitWait", "OCTOCODE_MAX_RATE_LIMIT_WAIT"],
                format!("fixed at {}", seconds(limits.max_rate_limit_wait)),
            ),
            (
                &["output.pagination", "OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH"],
                format!("fixed at {} characters", grouped(limits.auto_page_chars)),
            ),
            (
                &["storage.cloneCache", "OCTOCODE_CACHE_TTL_MS"],
                format!("{} h", clone.ttl.as_secs() / 3600),
            ),
            (
                &["storage.cloneCache", "OCTOCODE_MAX_CACHE_SIZE"],
                format!("{} GiB", clone.max_bytes >> 30),
            ),
            (
                &["storage.cloneCache", "OCTOCODE_MAX_CLONES"],
                format!("{} repositories", clone.max_clones),
            ),
            (
                &[
                    "classification.maxConcurrency",
                    "OCTOCODE_CLASSIFICATION_CONCURRENCY",
                ],
                format!("fixed at {} in-flight", limits.classification_concurrency),
            ),
            (
                &[
                    "classification.maxCandidates",
                    "OCTOCODE_CLASSIFICATION_MAX_CANDIDATES",
                ],
                format!("fixed at {} candidates", selection.max_candidates),
            ),
            (
                &[
                    "classification.maxChars",
                    "OCTOCODE_CLASSIFICATION_MAX_CHARS",
                ],
                format!("fixed at {} characters", grouped(selection.max_chars)),
            ),
            (
                &[
                    "classification.timeoutMs",
                    "OCTOCODE_CLASSIFICATION_TIMEOUT_MS",
                ],
                format!("fixed at {}", seconds(selection.timeout)),
            ),
        ];
        for (names, value) in cases {
            for name in names {
                let reason = crate::config::removed_reason(name)
                    .unwrap_or_else(|| panic!("{name} is a removed setting"));
                assert!(
                    states(reason, &value),
                    "{name}: {reason:?} does not state {value:?}"
                );
            }
        }
    }
}
