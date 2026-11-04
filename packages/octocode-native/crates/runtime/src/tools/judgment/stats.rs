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
            models: self
                .models
                .iter()
                .filter_map(|model| model.parse().ok())
                .collect(),
        }
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
