use super::*;
use crate::signatures::languages;
use serde_json::{Value, json};

fn run_pattern(src: &str, ext: &str, pattern: &str) -> Vec<StructuralMatch> {
    let matcher = compile_matcher(
        &lang(ext),
        &StructuralQuery::new(Some(pattern), None).expect("query"),
    )
    .expect("compile pattern");
    all_matches(&matcher, src)
        .expect("complete execution")
        .into_iter()
        .map(|m| m.matched)
        .collect()
}

fn run_rule(src: &str, ext: &str, rule: impl std::borrow::Borrow<Value>) -> Vec<StructuralMatch> {
    let matcher = compile_matcher(
        &lang(ext),
        &StructuralQuery::new(None, Some(rule.borrow())).expect("query"),
    )
    .expect("compile rule");
    all_matches(&matcher, src)
        .expect("complete execution")
        .into_iter()
        .map(|m| m.matched)
        .collect()
}

fn lang(ext: &str) -> AgLanguage {
    AgLanguage::new(languages::find_entry(ext).expect("test language should exist"))
}

mod grammars;
mod limits;
mod macros;
mod owned_core;
mod patterns;
mod recovery;
mod rule_features;
mod rules;
