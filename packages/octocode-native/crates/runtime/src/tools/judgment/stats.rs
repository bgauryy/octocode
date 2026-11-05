//! Classification usage aggregated over one tool call, reported in its result.

use std::collections::BTreeSet;

/// Classification usage aggregated over one tool call.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ClassificationUsage {
    pub calls: u64,
    pub known_usage_calls: u64,
    pub unknown_usage_calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub models: BTreeSet<String>,
    /// What the rest of the walk would bill, at the rate this page measured.
    pub remaining_input_tokens: u64,
}

impl ClassificationUsage {
    pub(crate) fn output(&self) -> octocode_contract_types::SelectionUsage {
        let count = |value| i64::try_from(value).unwrap_or(i64::MAX);
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
        }
    }

    /// The rest of a walk priced at this page's own rate: `judged` characters
    /// of states and questions billed these tokens, and `rest` remain.
    pub(crate) fn estimate_rest(&mut self, judged: usize, rest: usize) {
        self.remaining_input_tokens = u64::try_from(
            u128::from(self.input_tokens).saturating_mul(rest as u128) / judged.max(1) as u128,
        )
        .unwrap_or(u64::MAX);
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

    /// Fold one evaluation in: its provider requests and reported tokens.
    pub(crate) fn add(&mut self, outcome: &Result<super::Judged, super::ClassificationError>) {
        match outcome {
            Ok(judged) => {
                self.models.insert(judged.model.clone());
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
