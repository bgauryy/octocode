//! GitLab, GitHub and Bitbucket tokens.
//!
//! A contiguous slice of [`super::PATTERNS`]: its order is part of the
//! evaluation order.

use super::SecretPattern;

pub(super) const PATTERNS: &[SecretPattern] = &[
    SecretPattern {
        name: "gitlabPersonalAccessToken",
        regex: r###"glpat-[A-Za-z0-9_-]{20,}\b"###,
    },
    SecretPattern {
        name: "gitlabDeployToken",
        regex: r###"gldt-[A-Za-z0-9_-]{20}"###,
    },
    SecretPattern {
        name: "gitlabRunnerToken",
        regex: r###"glrt-[A-Za-z0-9_-]{20}"###,
    },
    SecretPattern {
        name: "gitlabCiJobToken",
        regex: r###"glcbt-[0-9a-zA-Z]{1,5}_[0-9a-zA-Z_-]{20}"###,
    },
    SecretPattern {
        name: "gitlabPipelineTriggerToken",
        regex: r###"glptt-[0-9a-f]{40}"###,
    },
    SecretPattern {
        name: "bitbucketAppPassword",
        regex: r###"ATBB[a-zA-Z0-9]{24}"###,
    },
    SecretPattern {
        name: "githubTokens",
        regex: r###"((?:ghp|gho|ghu|ghs|ghr|github_pat)_[a-zA-Z0-9_]{36,255})"###,
    },
    SecretPattern {
        name: "githubAppInstallationToken",
        regex: r###"ghs_[0-9a-zA-Z]{37}"###,
    },
    SecretPattern {
        name: "gitlabScimToken",
        regex: r###"glsoat-[0-9a-zA-Z_-]{20}"###,
    },
    SecretPattern {
        name: "gitlabFeatureFlagToken",
        regex: r###"glffct-[0-9a-zA-Z_-]{20}"###,
    },
    SecretPattern {
        name: "gitlabFeedToken",
        regex: r###"glft-[0-9a-zA-Z_-]{20}"###,
    },
    SecretPattern {
        name: "gitlabIncomingMailToken",
        regex: r###"glimt-[0-9a-zA-Z_-]{25}"###,
    },
    SecretPattern {
        name: "gitlabK8sAgentToken",
        regex: r###"glagent-[0-9a-zA-Z_-]{50}"###,
    },
    SecretPattern {
        name: "gitlabOAuthAppSecret",
        regex: r###"gloas-[0-9a-zA-Z_-]{64}"###,
    },
    SecretPattern {
        name: "gitlabSessionCookie",
        regex: r###"_gitlab_session=[0-9a-z]{32}"###,
    },
    SecretPattern {
        name: "bitbucketRepoToken",
        regex: r###"ATCTT3[a-zA-Z0-9]{24}"###,
    },
];
