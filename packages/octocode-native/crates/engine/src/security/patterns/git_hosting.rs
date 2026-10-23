//! GitLab, GitHub and Bitbucket tokens.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "gitlabPersonalAccessToken",
        file_context: None,
        regex: r###"\bglpat-[A-Za-z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "gitlabDeployToken",
        file_context: None,
        regex: r###"\bgldt-[A-Za-z0-9_-]{20}\b"###,
    },
    SecretPattern {
        name: "gitlabRunnerToken",
        file_context: None,
        regex: r###"\bglrt-[A-Za-z0-9_-]{20}\b"###,
    },
    SecretPattern {
        name: "gitlabCiJobToken",
        file_context: None,
        regex: r###"\bglcbt-[0-9a-zA-Z]{1,5}_[0-9a-zA-Z_-]{20}\b"###,
    },
    SecretPattern {
        name: "gitlabPipelineTriggerToken",
        file_context: None,
        regex: r###"\bglptt-[0-9a-f]{40}\b"###,
    },
    SecretPattern {
        name: "bitbucketAppPassword",
        file_context: None,
        regex: r###"\bATBB[a-zA-Z0-9]{24}\b"###,
    },
    SecretPattern {
        name: "githubTokens",
        file_context: None,
        regex: r###"\b((?:ghp|gho|ghu|ghs|ghr|github_pat)_[a-zA-Z0-9_]{36,255})\b"###,
    },
    SecretPattern {
        name: "githubFineGrainedToken",
        file_context: Some("(?:\\.env|config|settings|secrets)"),
        regex: r###"\bgithub_pat_[A-Za-z0-9_]{82}\b"###,
    },
    SecretPattern {
        name: "githubAppInstallationToken",
        file_context: None,
        regex: r###"\bghs_[0-9a-zA-Z]{37}\b"###,
    },
    SecretPattern {
        name: "gitlabScimToken",
        file_context: None,
        regex: r###"\bglsoat-[0-9a-zA-Z_-]{20}\b"###,
    },
    SecretPattern {
        name: "gitlabFeatureFlagToken",
        file_context: None,
        regex: r###"\bglffct-[0-9a-zA-Z_-]{20}\b"###,
    },
    SecretPattern {
        name: "gitlabFeedToken",
        file_context: None,
        regex: r###"\bglft-[0-9a-zA-Z_-]{20}\b"###,
    },
    SecretPattern {
        name: "gitlabIncomingMailToken",
        file_context: None,
        regex: r###"\bglimt-[0-9a-zA-Z_-]{25}\b"###,
    },
    SecretPattern {
        name: "gitlabK8sAgentToken",
        file_context: None,
        regex: r###"\bglagent-[0-9a-zA-Z_-]{50}\b"###,
    },
    SecretPattern {
        name: "gitlabOAuthAppSecret",
        file_context: None,
        regex: r###"\bgloas-[0-9a-zA-Z_-]{64}\b"###,
    },
    SecretPattern {
        name: "gitlabSessionCookie",
        file_context: None,
        regex: r###"_gitlab_session=[0-9a-z]{32}"###,
    },
    SecretPattern {
        name: "bitbucketRepoToken",
        file_context: None,
        regex: r###"\bATCTT3[a-zA-Z0-9]{24}\b"###,
    },
];
