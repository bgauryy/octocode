use super::*;
use crate::execution::cancel::CancellationCheck;
use crate::response::source_evidence;

pub(super) fn range(
    range: octocode_engine::text::analysis::ByteRange,
) -> Result<Value, GraphOutput> {
    source_evidence::project_range(range)
        .map(|range| json!(range))
        .map_err(|error| GraphOutput::error(error.code, error.message))
}

impl Graph {
    pub(super) fn origin(
        &self,
        reference: u32,
        cancel: &dyn CancellationCheck,
    ) -> Result<Value, GraphOutput> {
        if reference == NONE {
            return Ok(Value::Null);
        }
        let origin = self
            .t
            .sources
            .origin(reference, &mut self.source_budget.borrow_mut(), cancel)
            .map_err(|error| GraphOutput::error(error.code, error.message))?;
        let projected =
            source_evidence::project_origin(&origin, &mut self.source_budget.borrow_mut(), cancel)
                .map_err(|error| GraphOutput::error(error.code, error.message))?;
        Ok(json!(projected))
    }

    pub(super) fn close_sources(
        &self,
        output: &mut GraphOutput,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), GraphOutput> {
        let units = source_evidence::referenced_units(
            &output.value,
            &mut self.source_budget.borrow_mut(),
            cancel,
        )
        .map_err(|error| GraphOutput::error(error.code, error.message))?;
        if units.is_empty() {
            return Ok(());
        }
        let descriptors = source_evidence::project_descriptors(
            &self.t.sources.descriptors,
            units.iter().map(String::as_str),
            &mut self.source_budget.borrow_mut(),
            cancel,
        )
        .map_err(|error| GraphOutput::error(error.code, error.message))?;
        output.value["sourceEvidence"] = json!(descriptors);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::cancel::NeverCancel;

    #[test]
    fn graph_projection_overflow_returns_only_a_terminal_error() {
        let mut graph = Graph::default();
        graph.t.sources = super::super::super::format::evidence_tests::source_fixture();
        graph.source_budget = std::cell::RefCell::new(source_evidence::ProjectionBudget::new(
            source_evidence::ProjectionLimits {
                bytes: 1,
                records: 1,
            },
        ));
        let error = graph.origin(0, &NeverCancel).unwrap_err();
        assert_eq!(error.value["errorCode"], "responseTooLarge");
        assert!(error.value.get("sourceOrigin").is_none());
        assert!(error.value.get("sourceEvidence").is_none());
        assert!(error.value.get("results").is_none());
    }
}
