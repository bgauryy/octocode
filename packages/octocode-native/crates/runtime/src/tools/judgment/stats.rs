//! Classification usage aggregated over one tool call, reported in its result.

use octocode_engine::num::{saturating_i64, saturating_u64};
use std::collections::BTreeSet;

/// Classification usage aggregated over one tool call.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct ClassificationUsage {
    pub calls: u64,
    pub known_usage_calls: u64,
    pub unknown_usage_calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub models: BTreeSet<String>,
    /// What the rest of the walk would bill, at the rate this page measured.
    pub remaining_input_tokens: u64,
    /// Evaluations the default window refused and the long-context window
    /// answered, the input tokens they reported, how many reported usage for
    /// some long-window request, and how many did not for some (those tokens
    /// are unknown).
    pub long_context_retries: u64,
    pub long_context_input_tokens: u64,
    pub long_context_reported: u64,
    pub long_context_unreported: u64,
    /// This usage describes a replayed page: its billing was the original
    /// read's, and the notice says so.
    #[serde(default)]
    pub replay: bool,
}

impl ClassificationUsage {
    /// What a replay of this page reports: the work it did not bill again
    /// is zero, what describes the page stays (the models, the price of the
    /// rest of the walk, the long-context retries it took).
    pub(crate) fn replayed(&self) -> Self {
        Self {
            calls: 0,
            known_usage_calls: 0,
            unknown_usage_calls: 0,
            input_tokens: 0,
            output_tokens: 0,
            replay: true,
            ..self.clone()
        }
    }

    pub(crate) fn output(&self) -> octocode_contract_types::SelectionUsage {
        let count = |value| saturating_i64(value);
        octocode_contract_types::SelectionUsage {
            calls: count(self.calls),
            known_usage_calls: count(self.known_usage_calls),
            unknown_usage_calls: count(self.unknown_usage_calls),
            input_tokens: count(self.input_tokens),
            output_tokens: count(self.output_tokens),
            remaining_input_tokens_estimate: std::num::NonZeroU64::new(self.remaining_input_tokens),
            models: self
                .models
                .iter()
                .filter_map(|model| model.parse().ok())
                .collect(),
            replayed: self.replay.then_some(true),
        }
    }

    /// The rest of a walk priced at this page's own rate: `judged` characters
    /// of states and questions billed these tokens, and `rest` remain.
    pub(crate) fn estimate_rest(&mut self, judged: usize, rest: usize) {
        self.remaining_input_tokens = saturating_u64(
            u128::from(self.input_tokens).saturating_mul(rest as u128) / judged.max(1) as u128,
        );
    }

    /// Fold in `calls` provider requests, `known` of which reported their
    /// tokens. Retained directional totals are never called complete when
    /// a request did not report them; a reported zero is known usage.
    pub(crate) fn add_usage(
        &mut self,
        calls: u64,
        known: u64,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        let known = known.min(calls);
        self.calls = self.calls.saturating_add(calls);
        self.known_usage_calls = self.known_usage_calls.saturating_add(known);
        self.unknown_usage_calls = self.unknown_usage_calls.saturating_add(calls - known);
        self.input_tokens = self.input_tokens.saturating_add(input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(output_tokens);
    }

    /// What a row that billed long-context retries says of them, through
    /// its reader's own channel; `None` when none was retried.
    pub(crate) fn long_context_notice(&self) -> Option<String> {
        if self.long_context_retries == 0 {
            return None;
        }
        let retried =
            octocode_engine::text::counted(self.long_context_retries, "evaluation", "evaluations");
        let tokens = self.long_context_input_tokens;
        let unknown = self.long_context_unreported;
        // A replay billed nothing: the tokens are the original read's.
        let origin = if self.replay {
            " by the original read"
        } else {
            ""
        };
        Some(if unknown == 0 {
            format!(
                "{retried} refused in the default window retried in the long-context window (slower, less accurate): {tokens} input tokens billed{origin}. longContext:false refuses instead."
            )
        } else if self.long_context_reported == 0 {
            format!(
                "{retried} refused in the default window retried in the long-context window (slower, less accurate): input tokens billed unknown{origin} (the provider reported no usage). longContext:false refuses instead."
            )
        } else {
            format!(
                "{retried} refused in the default window retried in the long-context window (slower, less accurate): at least {tokens} input tokens billed{origin}; {unknown} of them reported no usage. longContext:false refuses instead."
            )
        })
    }

    /// Count `judged` among the long-context retries when it was one.
    pub(crate) fn add_retry(&mut self, judged: &super::Judged) {
        let Some(retry) = judged.long_context_retry else {
            return;
        };
        self.long_context_retries = self.long_context_retries.saturating_add(1);
        self.long_context_input_tokens = self
            .long_context_input_tokens
            .saturating_add(judged.input_tokens);
        // The refused request never reports usage, so any known one is the
        // long window's.
        if judged.known_usage_calls > 0 {
            self.long_context_reported = self.long_context_reported.saturating_add(1);
        }
        if retry.unreported_calls > 0 {
            self.long_context_unreported = self.long_context_unreported.saturating_add(1);
        }
    }

    /// Fold one evaluation in: its provider requests and reported tokens.
    pub(crate) fn add(&mut self, outcome: &Result<super::Judged, super::ClassificationError>) {
        match outcome {
            Ok(judged) => {
                self.models.insert(judged.model.clone());
                self.add_retry(judged);
                if judged.provider_calls > 0 {
                    self.add_usage(
                        judged.provider_calls,
                        judged.known_usage_calls,
                        judged.input_tokens,
                        judged.output_tokens,
                    );
                }
            }
            Err(error) if error.provider_calls > 0 => {
                let usage = error.usage();
                self.add_usage(
                    error.provider_calls,
                    usage.known_usage_calls,
                    usage.input_tokens,
                    usage.output_tokens,
                );
            }
            Err(_) => {}
        }
    }
}
