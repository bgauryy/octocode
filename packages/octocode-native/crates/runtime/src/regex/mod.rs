use regex::{Regex, RegexBuilder};

mod isolated;
pub use isolated::{
    ExecutionMetadata, IsolatedRegexEngine, IsolatedRegexLimits, MemoryConfinement,
};

pub const REGEX_WORKER_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegexExecutionClass {
    LinearInProcess,
    RequiresIsolatedEngine,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegexErrorCode {
    InvalidFlags,
    InvalidPattern,
    RequiresIsolatedEngine,
    InputTooLarge,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegexError {
    pub code: RegexErrorCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegexLimits {
    pub max_pattern_bytes: usize,
    pub max_input_bytes: usize,
    pub max_matches: usize,
}

impl Default for RegexLimits {
    fn default() -> Self {
        Self {
            max_pattern_bytes: 4_096,
            max_input_bytes: 10_000,
            max_matches: 10_000,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EcmaPattern {
    source: String,
    flags: String,
    global: bool,
    class: RegexExecutionClass,
    linear: Option<Regex>,
    limits: RegexLimits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct MatchRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRequest {
    pub version: u32,
    pub source: String,
    pub flags: String,
    pub input: String,
    pub operation: WorkerOperation,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum WorkerOperation {
    Find { max_matches: usize },
    ReplaceLiteral { replacement: String },
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerResponse {
    pub version: u32,
    pub ranges: Option<Vec<MatchRange>>,
    pub replaced: Option<String>,
    pub error: Option<String>,
}

impl EcmaPattern {
    pub fn compile(source: &str, flags: &str, limits: RegexLimits) -> Result<Self, RegexError> {
        if source.len() > limits.max_pattern_bytes {
            return Err(error(
                RegexErrorCode::InvalidPattern,
                "Regex pattern exceeds its byte limit",
            ));
        }
        validate_flags(flags)?;
        let global = flags.contains('g');
        let class = if requires_ecma_backtracking(source, flags) {
            RegexExecutionClass::RequiresIsolatedEngine
        } else {
            RegexExecutionClass::LinearInProcess
        };
        let linear = if class == RegexExecutionClass::LinearInProcess {
            Some(
                RegexBuilder::new(source)
                    .case_insensitive(flags.contains('i'))
                    .multi_line(flags.contains('m'))
                    .dot_matches_new_line(flags.contains('s'))
                    .build()
                    .map_err(|failure| {
                        error(RegexErrorCode::InvalidPattern, failure.to_string())
                    })?,
            )
        } else {
            None
        };
        Ok(Self {
            source: source.to_owned(),
            flags: flags.to_owned(),
            global,
            class,
            linear,
            limits,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn flags(&self) -> &str {
        &self.flags
    }
    pub fn execution_class(&self) -> RegexExecutionClass {
        self.class
    }

    pub fn find_ranges(&self, input: &str) -> Result<Vec<MatchRange>, RegexError> {
        self.check_input(input)?;
        let regex = self.linear()?;
        let take = if self.global {
            self.limits.max_matches
        } else {
            1
        };
        Ok(regex
            .find_iter(input)
            .take(take)
            .map(|item| MatchRange {
                start: item.start(),
                end: item.end(),
            })
            .collect())
    }

    pub fn replace_literal(&self, input: &str, replacement: &str) -> Result<String, RegexError> {
        self.check_input(input)?;
        let regex = self.linear()?;
        Ok(if self.global {
            regex
                .replace_all(input, regex::NoExpand(replacement))
                .into_owned()
        } else {
            regex
                .replace(input, regex::NoExpand(replacement))
                .into_owned()
        })
    }

    fn linear(&self) -> Result<&Regex, RegexError> {
        self.linear.as_ref().ok_or_else(|| {
            error(
                RegexErrorCode::RequiresIsolatedEngine,
                "ECMAScript-only regex requires an interruptible isolated engine",
            )
        })
    }

    fn check_input(&self, input: &str) -> Result<(), RegexError> {
        if input.len() > self.limits.max_input_bytes {
            Err(error(
                RegexErrorCode::InputTooLarge,
                "Regex input exceeds its byte limit",
            ))
        } else {
            Ok(())
        }
    }
}

fn validate_flags(flags: &str) -> Result<(), RegexError> {
    let mut seen = std::collections::HashSet::new();
    for flag in flags.chars() {
        if !matches!(flag, 'g' | 'i' | 'm' | 's' | 'u' | 'y' | 'd') || !seen.insert(flag) {
            return Err(error(
                RegexErrorCode::InvalidFlags,
                format!("Unsupported or duplicate JavaScript regex flag: {flag}"),
            ));
        }
    }
    Ok(())
}

fn requires_ecma_backtracking(source: &str, flags: &str) -> bool {
    flags.contains('y')
        || flags.contains('d')
        || source.contains("(?=")
        || source.contains("(?!")
        || source.contains("(?<=")
        || source.contains("(?<!")
        || has_backreference(source)
}

fn has_backreference(source: &str) -> bool {
    let bytes = source.as_bytes();
    bytes
        .windows(2)
        .any(|pair| pair[0] == b'\\' && pair[1].is_ascii_digit())
        || source.contains("\\k<")
}

fn error(code: RegexErrorCode, message: impl Into<String>) -> RegexError {
    RegexError {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executes_linear_ecma_subset_with_js_flags_and_literal_replacement() {
        let pattern =
            EcmaPattern::compile("token-[a-z]+", "gi", RegexLimits::default()).expect("pattern");
        assert_eq!(
            pattern
                .find_ranges("TOKEN-one token-two")
                .expect("ranges")
                .len(),
            2
        );
        assert_eq!(
            pattern
                .replace_literal("TOKEN-one token-two", "$SAFE")
                .expect("replace"),
            "$SAFE $SAFE"
        );
    }

    #[test]
    fn preserves_first_replacement_without_global_flag() {
        let pattern = EcmaPattern::compile("token", "i", RegexLimits::default()).expect("pattern");
        assert_eq!(
            pattern
                .replace_literal("TOKEN token", "x")
                .expect("replace"),
            "x token"
        );
    }

    #[test]
    fn refuses_ecma_backtracking_features_in_process() {
        for source in ["(?<=token=)x", r"(a+)\1", "a(?=b)"] {
            let pattern = EcmaPattern::compile(source, "g", RegexLimits::default())
                .expect("classified pattern");
            assert_eq!(
                pattern.execution_class(),
                RegexExecutionClass::RequiresIsolatedEngine
            );
            assert_eq!(
                pattern
                    .find_ranges("token=x aaaa ab")
                    .expect_err("must isolate")
                    .code,
                RegexErrorCode::RequiresIsolatedEngine
            );
        }
    }

    #[test]
    fn rejects_duplicate_flags_and_bounded_inputs() {
        assert_eq!(
            EcmaPattern::compile("x", "gg", RegexLimits::default())
                .expect_err("duplicate")
                .code,
            RegexErrorCode::InvalidFlags
        );
        let pattern = EcmaPattern::compile(
            "x",
            "",
            RegexLimits {
                max_input_bytes: 2,
                ..RegexLimits::default()
            },
        )
        .expect("pattern");
        assert_eq!(
            pattern.find_ranges("xxx").expect_err("bounded").code,
            RegexErrorCode::InputTooLarge
        );
    }
}
