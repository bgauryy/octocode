use sha2::{Digest, Sha256};

use super::*;

pub struct BoundSource<'a> {
    pub(super) templates: std::sync::Arc<SourceTemplates>,
    pub(super) sources: Vec<&'a AdmittedSource<'a>>,
    pub(super) representations: Vec<TextRepresentation>,
    pub(super) units: Vec<AnalysisUnit>,
}

impl<'a> BoundSource<'a> {
    pub(super) fn new(
        templates: std::sync::Arc<SourceTemplates>,
        sources: &[&'a AdmittedSource<'a>],
        budget: &mut MapBudget<'_>,
    ) -> Result<Self, SourceError> {
        budget.check()?;
        if templates.representations.is_empty() || templates.units.is_empty() {
            return Err(SourceError::InvalidMap("empty source templates"));
        }
        MapBudget::charge(
            &mut budget.usage.representations,
            templates.representations.len(),
            budget.limits.representations,
            "representations",
        )?;
        budget.allocate(templates.heap_bytes() + std::mem::size_of_val(sources))?;
        let mut bound = Self {
            templates: std::sync::Arc::clone(&templates),
            sources: sources.to_vec(),
            representations: Vec::new(),
            units: Vec::new(),
        };
        budget.allocate(templates.representations.len().saturating_mul(
            1 + std::mem::size_of::<Option<TextRepresentation>>()
                + std::mem::size_of::<TextRepresentation>(),
        ))?;
        budget.allocate(
            templates
                .units
                .len()
                .saturating_mul(std::mem::size_of::<AnalysisUnit>()),
        )?;
        bound.units = Vec::with_capacity(templates.units.len());
        let mut state = vec![0; templates.representations.len()];
        let mut resolved = vec![None; templates.representations.len()];
        for index in 0..templates.representations.len() {
            bound.resolve_representation(index, &mut state, &mut resolved, budget, 0)?;
        }
        bound.representations = resolved
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(SourceError::InvalidMap("unresolved representation"))?;
        for unit in &templates.units {
            bound.units.push(bound.resolve_unit(unit, budget)?);
        }
        Ok(bound)
    }

    fn resolve_representation(
        &self,
        index: usize,
        state: &mut [u8],
        resolved: &mut [Option<TextRepresentation>],
        budget: &mut MapBudget<'_>,
        depth: usize,
    ) -> Result<(), SourceError> {
        budget.depth(depth)?;
        let slot = state
            .get_mut(index)
            .ok_or(SourceError::InvalidMap("unknown representation"))?;
        match *slot {
            1 => return Err(SourceError::InvalidMap("cyclic source map")),
            2 => return Ok(()),
            _ => *slot = 1,
        }
        let template = &self.templates.representations[index];
        let mut inputs = Vec::new();
        match &template.content {
            RepresentationContent::Prefix { input, .. } => {
                budget.reserve(&mut inputs, 1)?;
                inputs.push(*input);
            }
            RepresentationContent::Mapped { segments, .. } => {
                for segment in segments {
                    budget.check()?;
                    for branch in &segment.branches {
                        budget.check()?;
                        match branch {
                            MapBranch::Exact { input, .. } => {
                                budget.reserve(&mut inputs, 1)?;
                                inputs.push(*input);
                            }
                            MapBranch::Derived { contributors } => {
                                for contribution in contributors {
                                    budget.check()?;
                                    if let MapContribution::Source { input, .. } = contribution {
                                        budget.reserve(&mut inputs, 1)?;
                                        inputs.push(*input);
                                    }
                                }
                            }
                            MapBranch::Unmapped(_) => {}
                        }
                    }
                }
            }
            RepresentationContent::Root { .. } => {}
        }
        for input in &inputs {
            self.resolve_representation(input.0 as usize, state, resolved, budget, depth + 1)?;
        }
        let handle = RepresentationHandle(
            u32::try_from(index).map_err(|_| SourceError::LimitExceeded("representations"))?,
        );
        self.validate_template(handle, template, budget)?;
        let mut input_ids = Vec::new();
        let mut source_ids = Vec::new();
        for input in inputs {
            let descriptor = resolved[input.0 as usize]
                .as_ref()
                .ok_or(SourceError::InvalidMap("input descriptor"))?;
            budget.reserve(&mut input_ids, 1)?;
            budget.allocate(descriptor.id.len())?;
            input_ids.push(descriptor.id.clone());
            for id in &descriptor.source_ids {
                if !source_ids.contains(id) {
                    budget.reserve(&mut source_ids, 1)?;
                    budget.allocate(id.len())?;
                    source_ids.push(id.clone());
                }
            }
        }
        if let RepresentationContent::Root { source_index, .. } = &template.content {
            let source = self
                .sources
                .get(*source_index as usize)
                .ok_or(SourceError::InvalidMap("missing raw input"))?;
            if let Some(snapshot) = &source.snapshot {
                budget.reserve(&mut source_ids, 1)?;
                budget.reserve(&mut input_ids, 1)?;
                budget.allocate(snapshot.id.len() * 2)?;
                source_ids.push(snapshot.id.clone());
                input_ids.push(snapshot.id.clone());
            } else {
                budget.reserve(&mut input_ids, 1)?;
                budget.allocate(14)?;
                input_ids.push("decoded-only:1".to_owned());
            }
        }
        let byte_length = self.length(handle, budget, 0)?;
        let mut hash = Sha256::new();
        self.chunks(
            handle,
            ByteRange::new(0, byte_length),
            &mut |chunk| hash.update(chunk.as_bytes()),
            budget,
            0,
        )?;
        budget.allocate(128 + template.producer.id.len() + template.producer.version.len())?;
        let source_digest = hex::encode(hash.finalize());
        let id = model::identity(&(
            "representation:1",
            template,
            &input_ids,
            &source_digest,
            byte_length,
        ))?;
        resolved[index] = Some(TextRepresentation {
            id,
            source_digest,
            byte_length,
            producer: template.producer.clone(),
            source_ids,
        });
        state[index] = 2;
        Ok(())
    }

    pub(super) fn length(
        &self,
        input: RepresentationHandle,
        budget: &mut MapBudget<'_>,
        depth: usize,
    ) -> Result<u32, SourceError> {
        budget.depth(depth)?;
        let template = self
            .templates
            .representations
            .get(input.0 as usize)
            .ok_or(SourceError::InvalidMap("representation"))?;
        match &template.content {
            RepresentationContent::Root { byte_length, .. } => Ok(*byte_length),
            RepresentationContent::Mapped { text, .. } => u32::try_from(text.len())
                .map_err(|_| SourceError::LimitExceeded("representation bytes")),
            RepresentationContent::Prefix { end_byte, tail, .. } => end_byte
                .checked_add(
                    u32::try_from(tail.len())
                        .map_err(|_| SourceError::LimitExceeded("tail bytes"))?,
                )
                .ok_or(SourceError::InvalidMap("prefix overflow")),
        }
    }
    pub(super) fn validate_range(
        &self,
        input: RepresentationHandle,
        range: ByteRange,
        budget: &mut MapBudget<'_>,
    ) -> Result<(), SourceError> {
        if range.start_byte > range.end_byte || range.end_byte > self.length(input, budget, 0)? {
            return Err(SourceError::InvalidMap("range bounds"));
        }
        self.chunks(input, range, &mut |_| {}, budget, 0)
    }
    pub(super) fn chunks(
        &self,
        input: RepresentationHandle,
        range: ByteRange,
        visit: &mut dyn FnMut(&str),
        budget: &mut MapBudget<'_>,
        depth: usize,
    ) -> Result<(), SourceError> {
        budget.depth(depth)?;
        match &self
            .templates
            .representations
            .get(input.0 as usize)
            .ok_or(SourceError::InvalidMap("representation"))?
            .content
        {
            RepresentationContent::Root { source_index, .. } => {
                let source = self
                    .sources
                    .get(*source_index as usize)
                    .ok_or(SourceError::InvalidMap("source"))?;
                visit(
                    source
                        .text()
                        .get(range.start_byte as usize..range.end_byte as usize)
                        .ok_or(SourceError::InvalidMap("scalar boundary"))?,
                );
            }
            RepresentationContent::Mapped { text, .. } => visit(
                text.get(range.start_byte as usize..range.end_byte as usize)
                    .ok_or(SourceError::InvalidMap("scalar boundary"))?,
            ),
            RepresentationContent::Prefix {
                input,
                end_byte,
                tail,
            } => {
                if range.start_byte <= *end_byte {
                    self.chunks(
                        *input,
                        ByteRange::new(range.start_byte, range.end_byte.min(*end_byte)),
                        visit,
                        budget,
                        depth + 1,
                    )?;
                }
                if range.end_byte > *end_byte {
                    visit(
                        tail.get(
                            range.start_byte.saturating_sub(*end_byte) as usize
                                ..(range.end_byte - end_byte) as usize,
                        )
                        .ok_or(SourceError::InvalidMap("tail boundary"))?,
                    );
                }
            }
        }
        Ok(())
    }
}
