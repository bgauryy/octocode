//! Facts across the rows of one call: where the yes answers sit, offered
//! once over every source that said yes; and a question whose every answer
//! is unsure, said once.
use super::{Row, recovery};
use serde_json::{Map, Value};

/// A noul answer in this range is unsure ("0.3-0.7 is unsure").
const UNSURE: std::ops::RangeInclusive<f64> = 0.3..=0.7;

/// The fewest judgments of one question whose unsure answers point at the
/// question rather than at the sources.
const UNSURE_JUDGMENTS: usize = 3;

/// A source and its resource key (none for a `source` row).
type Keyed = (Option<String>, Value);

/// One source with a noul yes.
struct Yes {
    row: usize,
    /// The questions that said yes.
    questions: Map<String, Value>,
    source: Keyed,
}

/// Every noul answer of one question (key and question), with the first row
/// that holds one.
struct Nouls {
    key: String,
    question: Value,
    row: usize,
    values: Vec<f64>,
}

/// One sections call over the sources whose yes questions agree, on the
/// first of their rows.
struct Lead {
    row: usize,
    questions: Map<String, Value>,
    sources: Vec<Keyed>,
}

/// What one call's judgments say together.
#[derive(Default)]
pub(super) struct Across {
    yes: Vec<Yes>,
    nouls: Vec<Nouls>,
}

impl Across {
    /// Record the `answers` row `index` gave over `source` (none for literal
    /// content).
    pub(super) fn judged(
        &mut self,
        index: usize,
        row: &Row,
        source: Option<&Value>,
        key: Option<&str>,
        answers: &[Value],
    ) {
        let mut yes = Map::new();
        for ((key, question), answer) in row.keys.iter().zip(&row.questions).zip(answers) {
            let Some(noul) = answer["noul"].as_f64().filter(|_| answer["type"] == "noul") else {
                continue;
            };
            if noul >= 0.5 {
                yes.insert(key.clone(), question.clone());
            }
            match self
                .nouls
                .iter_mut()
                .find(|known| known.key == *key && known.question == *question)
            {
                Some(known) => known.values.push(noul),
                None => self.nouls.push(Nouls {
                    key: key.clone(),
                    question: question.clone(),
                    row: index,
                    values: vec![noul],
                }),
            }
        }
        if let Some(source) = source
            && !yes.is_empty()
        {
            self.yes.push(Yes {
                row: index,
                questions: yes,
                source: (key.map(str::to_owned), source.clone()),
            });
        }
    }

    /// Put the call's facts on its result rows (`rows[i]` is row `i`'s data).
    /// Sources whose yes questions, context and model agree share one
    /// sections call, on the first of their rows; a source is named once.
    pub(super) fn attach(self, rows: &mut [Value], parsed: &[Row], can_resume: bool) {
        let mut leads: Vec<Lead> = Vec::new();
        for Yes {
            row: index,
            questions,
            source,
        } in self.yes
        {
            let same = |lead: &Lead| {
                lead.questions == questions
                    && parsed[lead.row].context == parsed[index].context
                    && parsed[lead.row].model == parsed[index].model
            };
            match leads.iter_mut().find(|lead| same(lead)) {
                Some(lead) if lead.sources.iter().any(|(_, known)| *known == source.1) => {}
                Some(lead) => lead.sources.push(source),
                None => leads.push(Lead {
                    row: index,
                    questions,
                    sources: vec![source],
                }),
            }
        }
        for lead in leads {
            let row = &parsed[lead.row];
            if let Some(call) = recovery::sections_over(
                lead.questions,
                row.context.as_deref(),
                row.model.as_deref(),
                lead.sources,
                can_resume,
            ) {
                crate::response::result::attach_related(&mut rows[lead.row], call);
            }
        }
        for Nouls {
            key, row, values, ..
        } in self.nouls
        {
            if values.len() < UNSURE_JUDGMENTS || !values.iter().all(|noul| UNSURE.contains(noul)) {
                continue;
            }
            let low = values.iter().copied().fold(f64::INFINITY, f64::min);
            let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            crate::response::result::add_hints(&mut rows[row], [unsure_hint(&key, low, high)]);
        }
    }
}

/// What a question whose every answer is unsure says.
fn unsure_hint(key: &str, low: f64, high: f64) -> String {
    format!(
        "Every answer to `{key}` is unsure ({low:.2}–{high:.2}): the question may be vague; ask one observable fact, or a choice with named options."
    )
}
