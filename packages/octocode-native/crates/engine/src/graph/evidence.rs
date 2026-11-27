//! Syntax evidence retained independently of semantic resolution.

use serde::{Deserialize, Serialize};

use super::GraphRange;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum GraphParseScope {
    Occurrence,
    Declaration,
    EnclosingDeclaration,
    Document,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum GraphRecoveryReason {
    SyntaxRecovery,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum GraphAssessmentMissingReason {
    AssessmentUnavailable,
}

/// Parser acceptance assesses syntax only. It establishes neither a binding
/// nor a successful compilation. Missing assessments never default to parsed.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum GraphParseAssessment {
    Parsed {
        scope: GraphParseScope,
        range: GraphRange,
    },
    Recovered {
        scope: GraphParseScope,
        reason: GraphRecoveryReason,
        range: GraphRange,
    },
    NotAssessed {
        reason: GraphAssessmentMissingReason,
    },
}

impl Default for GraphParseAssessment {
    fn default() -> Self {
        Self::NotAssessed {
            reason: GraphAssessmentMissingReason::AssessmentUnavailable,
        }
    }
}

impl GraphParseAssessment {
    pub fn parsed(range: GraphRange) -> Self {
        Self::Parsed {
            scope: GraphParseScope::Declaration,
            range,
        }
    }

    pub fn recovered(scope: GraphParseScope, range: GraphRange) -> Self {
        Self::Recovered {
            scope,
            reason: GraphRecoveryReason::SyntaxRecovery,
            range,
        }
    }
}
