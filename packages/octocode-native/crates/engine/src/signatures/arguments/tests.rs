use crate::graph::*;
use crate::signatures::{extract_graph_facts_with_metadata_inner, languages};

fn facts(source: &str, extension: &str) -> GraphFactsDocument {
    extract_graph_facts_with_metadata_inner(source, &format!("fixture.{extension}"))
        .expect("admitted grammar")
        .facts
}

fn slots(facts: &GraphFactsDocument) -> Vec<&GraphArgumentSlot> {
    facts
        .argument_sites
        .iter()
        .flat_map(|site| &site.groups)
        .flat_map(|group| match &group.observation {
            GraphArgumentGroupObservation::Slots { slots } => slots.as_slice(),
            GraphArgumentGroupObservation::Unsupported { .. } => &[],
        })
        .collect()
}

fn positive(adapter: super::ArgumentAdapter) -> &'static str {
    use super::ArgumentAdapter as A;
    match adapter {
        A::JavaScript => {
            "function handler(){} function main(){ consume(handler, 7, nested(), () => handler); }"
        }
        A::Python => {
            "def handler(): pass\ndef main():\n consume(handler, 7, nested(), lambda: handler)\n"
        }
        #[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
        A::CFamily => positive(A::C),
        A::C => "void handler() {} void main() { consume(handler, 7, nested(), handler); }",
        A::Go => {
            "package p\nfunc handler() {}\nfunc main(){ consume(handler, 7, nested(), func(){}) }"
        }
        A::Java => {
            "class Box { void handler() {} void main() { consume(handler, 7, nested(), () -> handler()); } }"
        }
        A::Rust => "fn handler() {} fn main() { consume(handler, 7, nested(), || handler()); }",
        #[cfg(feature = "tree-sitter-c-sharp")]
        A::CSharp => {
            "class Box { void handler() {} void Main() { consume(handler, 7, nested(), () => handler()); } }"
        }
        A::Scala => {
            "object Box { def handler() = (); def main() = consume(handler, 7, nested(), () => handler()) }"
        }
    }
}

mod conformance;
mod ranges;
mod scope;
