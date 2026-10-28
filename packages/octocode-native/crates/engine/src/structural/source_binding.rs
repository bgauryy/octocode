use super::types::{StructuralDetailedMatch, StructuralMatch};
use crate::text::analysis::{
    AdmittedSource, BoundSource, MapBudget, MapLimits, SourceDescriptors, SourceError,
    SourceTemplates, UnitHandle,
};
use std::{sync::Arc, time::Instant};

struct Frame<'a> {
    source: BoundSource<'a>,
    descriptors: Arc<SourceDescriptors>,
}
/// Binds immutable match templates to the bytes already admitted by the caller.
pub struct SourceBinder<'a> {
    admitted: &'a AdmittedSource<'a>,
    budget: MapBudget<'a>,
    frames: Vec<(usize, Frame<'a>)>,
}
impl<'a> SourceBinder<'a> {
    pub fn new(
        admitted: &'a AdmittedSource<'a>,
        limits: MapLimits,
        deadline: Instant,
        stop: &'a (dyn Fn() -> bool + Sync),
    ) -> Self {
        Self::with_budget(admitted, MapBudget::new(limits, deadline, stop))
    }
    pub fn with_budget(admitted: &'a AdmittedSource<'a>, budget: MapBudget<'a>) -> Self {
        Self {
            admitted,
            budget,
            frames: Vec::new(),
        }
    }
    pub fn usage(&self) -> crate::text::analysis::MapUsage {
        self.budget.usage
    }
    fn frame(&mut self, templates: &Arc<SourceTemplates>) -> Result<usize, SourceError> {
        let key = Arc::as_ptr(templates) as usize;
        match self.frames.binary_search_by_key(&key, |(key, _)| *key) {
            Ok(index) => Ok(index),
            Err(index) => {
                let source = templates.bind_shared(self.admitted, &mut self.budget)?;
                self.budget
                    .allocate(templates.units.len() * std::mem::size_of::<UnitHandle>())?;
                let units: Vec<_> = (0..templates.units.len())
                    .map(|i| UnitHandle(i as u32))
                    .collect();
                self.budget.allocate(
                    std::mem::size_of::<SourceDescriptors>() + 2 * std::mem::size_of::<usize>(),
                )?;
                let descriptors = Arc::new(source.descriptors_for_units(&units, &mut self.budget)?);
                self.budget.reserve(&mut self.frames, 1)?;
                self.frames.insert(
                    index,
                    (
                        key,
                        Frame {
                            source,
                            descriptors,
                        },
                    ),
                );
                Ok(index)
            }
        }
    }
    pub fn bind_match(&mut self, matched: &mut StructuralDetailedMatch) -> Result<(), SourceError> {
        let key = self.frame(&matched.source_templates)?;
        let frame = &self.frames[key].1;
        matched.source_origin = Some(
            frame
                .source
                .project(matched.source_span, &mut self.budget)?,
        );
        matched.header_origin = matched
            .header_span
            .map(|span| frame.source.project(span, &mut self.budget))
            .transpose()?;
        for capture in matched.metavar_ranges.values_mut().flatten() {
            capture.source_origin = Some(
                frame
                    .source
                    .project(capture.source_span, &mut self.budget)?,
            );
        }
        matched.source_descriptors = Some(Arc::clone(&frame.descriptors));
        Ok(())
    }
    pub(super) fn bind_raw_match(
        &mut self,
        matched: &mut StructuralMatch,
    ) -> Result<(), SourceError> {
        let key = self.frame(&matched.source_templates)?;
        let frame = &self.frames[key].1;
        matched.source_origin = Some(
            frame
                .source
                .project(matched.source_span, &mut self.budget)?,
        );
        matched.header_origin = matched
            .header_span
            .map(|span| frame.source.project(span, &mut self.budget))
            .transpose()?;
        for capture in matched.metavar_ranges.values_mut().flatten() {
            capture.source_origin = Some(
                frame
                    .source
                    .project(capture.source_span, &mut self.budget)?,
            );
        }
        matched.source_descriptors = Some(Arc::clone(&frame.descriptors));
        Ok(())
    }
}
