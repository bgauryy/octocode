use super::{Candidate, SelectionUnit};
use octocode_engine::text::identifier_words;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};

// Ranking only schedules assessment. It never labels evidence or changes its source ordinal.
pub(super) fn order(candidates: &[Candidate], query: &Value, unit: SelectionUnit) -> Vec<usize> {
    let source_order = || (0..candidates.len()).collect();
    if unit != SelectionUnit::Chunks {
        return source_order();
    }
    let Some(predicates) = query["classification"]["predicates"].as_array() else {
        return source_order();
    };
    let queries = predicates
        .iter()
        .filter_map(Value::as_str)
        .map(identifier_words)
        .collect::<Vec<_>>();
    let vocabulary = queries.iter().flatten().cloned().collect::<BTreeSet<_>>();
    if vocabulary.is_empty() {
        return source_order();
    }
    let terms = vocabulary
        .into_iter()
        .enumerate()
        .map(|(index, term)| (term, index))
        .collect::<HashMap<_, _>>();
    let mut frequencies = vec![0usize; terms.len()];
    let documents = candidates
        .iter()
        .map(|candidate| {
            let content = candidate
                .state
                .as_ref()
                .and_then(|state| state["content"].as_str())
                .unwrap_or_default();
            let words = identifier_words(content);
            let mut counts = vec![0usize; terms.len()];
            for word in &words {
                if let Some(&index) = terms.get(word) {
                    counts[index] += 1;
                }
            }
            for (index, &count) in counts.iter().enumerate() {
                frequencies[index] += usize::from(count > 0);
            }
            (words.len(), counts)
        })
        .collect::<Vec<_>>();
    let average = documents
        .iter()
        .map(|(length, _)| *length as f64)
        .sum::<f64>()
        / documents.len().max(1) as f64;
    if average == 0.0 {
        return source_order();
    }
    let rankings = queries
        .iter()
        .map(|query| {
            let query = query
                .iter()
                .filter_map(|word| terms.get(word))
                .copied()
                .collect::<BTreeSet<_>>();
            let mut scores = documents
                .iter()
                .enumerate()
                .map(|(ordinal, (length, counts))| {
                    let score = query
                        .iter()
                        .map(|&term| {
                            let frequency = frequencies[term] as f64;
                            let inverse = (1.0
                                + (documents.len() as f64 - frequency + 0.5) / (frequency + 0.5))
                                .ln();
                            let count = counts[term] as f64;
                            inverse * count * 2.2
                                / (count + 1.2 * (0.25 + 0.75 * *length as f64 / average))
                        })
                        .sum::<f64>();
                    (ordinal, score)
                })
                .collect::<Vec<_>>();
            scores.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            scores
        })
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    let mut order = Vec::with_capacity(candidates.len());
    for rank in 0..candidates.len() {
        for ranking in &rankings {
            let ordinal = ranking[rank].0;
            if seen.insert(ordinal) {
                order.push(ordinal);
            }
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_classifier_gets_an_admission_turn_and_ties_keep_source_order() {
        let candidates = ["plain", "socket", "socket", "timer"]
            .map(|content| Candidate::new(Some(json!({"content":content}))));
        let query = json!({"classification":{"predicates":["socket", "timer"]}});
        assert_eq!(
            order(&candidates, &query, SelectionUnit::Chunks),
            [1, 3, 2, 0]
        );
        assert_eq!(
            order(&candidates, &query, SelectionUnit::Files),
            [0, 1, 2, 3]
        );
        assert_eq!(
            order(
                &candidates,
                &json!({"classification":{"predicates":["absent"]}}),
                SelectionUnit::Chunks
            ),
            [0, 1, 2, 3]
        );
    }
}
