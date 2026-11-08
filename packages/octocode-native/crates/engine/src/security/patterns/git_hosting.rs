//! GitLab, GitHub and Bitbucket tokens.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "gitlabPersonalAccessToken",
        file_context: None,
        regex: r###"glpat-[A-Za-z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "gitlabDeployToken",
        file_context: None,
        regex: r###"gldt-[A-Za-z0-9_-]{20}"###,
    },
    SecretPattern {
        name: "gitlabRunnerToken",
        file_context: None,
        regex: r###"glrt-[A-Za-z0-9_-]{20}"###,
    },
    SecretPattern {
        name: "gitlabCiJobToken",
        file_context: None,
        regex: r###"glcbt-[0-9a-zA-Z]{1,5}_[0-9a-zA-Z_-]{20}"###,
    },
    SecretPattern {
        name: "gitlabPipelineTriggerToken",
        file_context: None,
        regex: r###"glptt-[0-9a-f]{40}"###,
    },
    SecretPattern {
        name: "bitbucketAppPassword",
        file_context: None,
        regex: r###"ATBB[a-zA-Z0-9]{24}"###,
    },
    SecretPattern {
        name: "githubTokens",
        file_context: None,
        regex: r###"((?:ghp|gho|ghu|ghs|ghr|github_pat)_[a-zA-Z0-9_]{36,255})"###,
    },
    SecretPattern {
        name: "githubFineGrainedToken",
        file_context: Some("(?:\\.env|config|settings|secrets)"),
        regex: r###"github_pat_[A-Za-z0-9_]{82}"###,
    },
    SecretPattern {
        name: "githubAppInstallationToken",
        file_context: None,
        regex: r###"ghs_[0-9a-zA-Z]{37}"###,
    },
    SecretPattern {
        name: "gitlabScimToken",
        file_context: None,
        regex: r###"glsoat-[0-9a-zA-Z_-]{20}"###,
    },
    SecretPattern {
        name: "gitlabFeatureFlagToken",
        file_context: None,
        regex: r###"glffct-[0-9a-zA-Z_-]{20}"###,
    },
    SecretPattern {
        name: "gitlabFeedToken",
        file_context: None,
        regex: r###"glft-[0-9a-zA-Z_-]{20}"###,
    },
    SecretPattern {
        name: "gitlabIncomingMailToken",
        file_context: None,
        regex: r###"glimt-[0-9a-zA-Z_-]{25}"###,
    },
    SecretPattern {
        name: "gitlabK8sAgentToken",
        file_context: None,
        regex: r###"glagent-[0-9a-zA-Z_-]{50}"###,
    },
    SecretPattern {
        name: "gitlabOAuthAppSecret",
        file_context: None,
        regex: r###"gloas-[0-9a-zA-Z_-]{64}"###,
    },
    SecretPattern {
        name: "gitlabSessionCookie",
        file_context: None,
        regex: r###"_gitlab_session=[0-9a-z]{32}"###,
    },
    SecretPattern {
        name: "bitbucketRepoToken",
        file_context: None,
        regex: r###"ATCTT3[a-zA-Z0-9]{24}"###,
    },
];
