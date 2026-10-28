use super::*;

impl BoundSource<'_> {
    pub fn project(
        &self,
        span: ObservedSpan,
        budget: &mut MapBudget<'_>,
    ) -> Result<OriginProjection, SourceError> {
        let unit = self
            .templates
            .units
            .get(span.unit.0 as usize)
            .ok_or(SourceError::InvalidMap("unknown unit"))?;
        self.validate_range(unit.representation, span.range, budget)?;
        let mut points = Vec::new();
        budget.reserve(&mut points, 2)?;
        points.extend([span.range.start_byte, span.range.end_byte]);
        self.boundaries(unit.representation, span.range, &mut points, budget, 0)?;
        for region in &unit.regions {
            budget.reserve(&mut points, 2)?;
            points.extend(
                [region.start_byte, region.end_byte]
                    .into_iter()
                    .filter(|p| *p > span.start_byte && *p < span.end_byte),
            );
        }
        points.sort_unstable();
        points.dedup();
        budget.allocate(
            points
                .len()
                .saturating_mul(std::mem::size_of::<ByteRange>()),
        )?;
        let ranges: Vec<_> = if span.range.is_empty() {
            vec![span.range]
        } else {
            points
                .windows(2)
                .map(|p| ByteRange::new(p[0], p[1]))
                .collect()
        };
        budget.allocate(ranges.len() * std::mem::size_of::<OriginSegment>())?;
        let length = self.length(unit.representation, budget, 0)?;
        let mut segments = Vec::with_capacity(ranges.len());
        for range in ranges {
            budget.segment()?;
            let mut included = false;
            for region in &unit.regions {
                budget.check()?;
                included = if length == 0 {
                    *region == range
                } else if range.is_empty() && range.start_byte == length {
                    region.start_byte < length && region.end_byte == length
                } else {
                    region.start_byte <= range.start_byte && range.start_byte < region.end_byte
                };
                if included {
                    break;
                }
            }
            segments.push(OriginSegment {
                source_range: range,
                analysis: if included {
                    AnalysisMembership::Included
                } else {
                    AnalysisMembership::Excluded
                },
                branches: self.branches(unit.representation, range, budget, 0)?,
            });
        }
        budget.allocate(self.units[span.unit.0 as usize].id.len())?;
        Ok(OriginProjection {
            analysis_unit_id: self.units[span.unit.0 as usize].id.clone(),
            segments,
        })
    }

    pub fn project_size(
        &self,
        span: ObservedSpan,
        budget: &mut MapBudget<'_>,
        price: impl FnOnce(&OriginProjection) -> usize,
    ) -> Result<usize, SourceError> {
        budget
            .temporary_size(|temporary| self.project(span, temporary).map(|origin| price(&origin)))
    }

    fn boundaries(
        &self,
        input: RepresentationHandle,
        range: ByteRange,
        points: &mut Vec<u32>,
        budget: &mut MapBudget<'_>,
        depth: usize,
    ) -> Result<(), SourceError> {
        budget.depth(depth)?;
        match &self.templates.representations[input.0 as usize].content {
            RepresentationContent::Root { .. } => {}
            RepresentationContent::Prefix {
                input, end_byte, ..
            } => {
                if range.start_byte < *end_byte && *end_byte < range.end_byte {
                    budget.reserve(points, 1)?;
                    points.push(*end_byte);
                }
                if range.start_byte < *end_byte {
                    self.boundaries(
                        *input,
                        ByteRange::new(range.start_byte, range.end_byte.min(*end_byte)),
                        points,
                        budget,
                        depth + 1,
                    )?;
                }
            }
            RepresentationContent::Mapped { segments, .. } => {
                for segment in segments {
                    budget.check()?;
                    let start = range.start_byte.max(segment.range.start_byte);
                    let end = range.end_byte.min(segment.range.end_byte);
                    if start >= end {
                        continue;
                    }
                    budget.reserve(points, 2)?;
                    points.extend([start, end]);
                    for branch in &segment.branches {
                        if let MapBranch::Exact {
                            input,
                            range: source,
                        } = branch
                        {
                            let mut child = Vec::new();
                            let mapped = ByteRange::new(
                                source.start_byte + start - segment.range.start_byte,
                                source.start_byte + end - segment.range.start_byte,
                            );
                            self.boundaries(*input, mapped, &mut child, budget, depth + 1)?;
                            budget.reserve(points, child.len())?;
                            points.extend(
                                child
                                    .into_iter()
                                    .map(|p| segment.range.start_byte + p - source.start_byte),
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn branches(
        &self,
        input: RepresentationHandle,
        range: ByteRange,
        budget: &mut MapBudget<'_>,
        depth: usize,
    ) -> Result<Vec<OriginBranch>, SourceError> {
        let left_anchor = range.is_empty()
            && range.start_byte != 0
            && range.start_byte == self.length(input, budget, depth)?;
        self.branches_at(input, range, left_anchor, budget, depth)
    }

    fn branches_at(
        &self,
        input: RepresentationHandle,
        range: ByteRange,
        left_anchor: bool,
        budget: &mut MapBudget<'_>,
        depth: usize,
    ) -> Result<Vec<OriginBranch>, SourceError> {
        budget.depth(depth)?;
        let mut result = Vec::new();
        match &self.templates.representations[input.0 as usize].content {
            RepresentationContent::Root { source_index, .. } => {
                budget.reserve(&mut result, 1)?;
                budget.allocate(
                    self.sources[*source_index as usize]
                        .snapshot
                        .as_ref()
                        .map_or(0, |s| s.id.len()),
                )?;
                result.push(self.sources[*source_index as usize].project(range)?);
            }
            RepresentationContent::Prefix {
                input,
                end_byte,
                tail,
            } => {
                let copied = range.end_byte <= *end_byte
                    && !(range.is_empty()
                        && range.start_byte == *end_byte
                        && !tail.is_empty()
                        && !left_anchor);
                if copied {
                    return self.branches_at(*input, range, left_anchor, budget, depth + 1);
                }
                budget.reserve(&mut result, 1)?;
                result.push(OriginBranch::Unmapped(UnmappedReason::Synthetic));
            }
            RepresentationContent::Mapped { text, segments } => {
                let segment = segments
                    .iter()
                    .find(|segment| contains(segment.range, range, text.len() as u32, left_anchor))
                    .ok_or(SourceError::InvalidMap("projection gap"))?;
                for branch in &segment.branches {
                    match branch {
                        MapBranch::Exact {
                            input,
                            range: source,
                        } => {
                            let range = ByteRange::new(
                                source.start_byte + range.start_byte - segment.range.start_byte,
                                source.start_byte + range.end_byte - segment.range.start_byte,
                            );
                            let branches =
                                self.branches_at(*input, range, left_anchor, budget, depth + 1)?;
                            budget.reserve(&mut result, branches.len())?;
                            result.extend(branches);
                        }
                        MapBranch::Unmapped(reason) => {
                            budget.reserve(&mut result, 1)?;
                            result.push(OriginBranch::Unmapped(*reason));
                        }
                        MapBranch::Derived { contributors } => {
                            budget.allocate(std::mem::size_of::<Vec<Contribution>>())?;
                            let mut alternatives = vec![Vec::new()];
                            for contributor in contributors {
                                match contributor {
                                    MapContribution::Unmapped(reason) => {
                                        budget.allocate(std::mem::size_of::<Contribution>())?;
                                        append_choices(
                                            &mut alternatives,
                                            &[vec![Contribution::Unmapped(*reason)]],
                                            budget,
                                        )?;
                                    }
                                    MapContribution::Source { input, range } => {
                                        let mut points = Vec::new();
                                        budget.reserve(&mut points, 2)?;
                                        points.extend([range.start_byte, range.end_byte]);
                                        self.boundaries(
                                            *input,
                                            *range,
                                            &mut points,
                                            budget,
                                            depth + 1,
                                        )?;
                                        points.sort_unstable();
                                        points.dedup();
                                        if range.is_empty() {
                                            budget.reserve(&mut points, 1)?;
                                            points.push(range.end_byte);
                                        }
                                        for pair in points.windows(2) {
                                            let branches = self.branches(
                                                *input,
                                                ByteRange::new(pair[0], pair[1]),
                                                budget,
                                                depth + 1,
                                            )?;
                                            budget.allocate(branches.len().saturating_mul(
                                                std::mem::size_of::<Vec<Contribution>>()
                                                    + std::mem::size_of::<Contribution>(),
                                            ))?;
                                            let choices: Vec<_> = branches
                                                .into_iter()
                                                .map(|branch| match branch {
                                                    OriginBranch::Copied(origin)
                                                    | OriginBranch::Transcoded(origin) => {
                                                        vec![Contribution::Source(origin)]
                                                    }
                                                    OriginBranch::Derived(contributors) => {
                                                        contributors
                                                    }
                                                    OriginBranch::Unmapped(reason) => {
                                                        vec![Contribution::Unmapped(reason)]
                                                    }
                                                })
                                                .collect();
                                            append_choices(&mut alternatives, &choices, budget)?;
                                        }
                                    }
                                }
                            }
                            budget.reserve(&mut result, alternatives.len())?;
                            result.extend(alternatives.into_iter().map(OriginBranch::Derived));
                        }
                    }
                }
            }
        }
        for _ in &result {
            budget.branch()?;
        }
        Ok(result)
    }
}

fn contains(segment: ByteRange, request: ByteRange, length: u32, left_anchor: bool) -> bool {
    if length == 0 {
        return segment == request;
    }
    if request.is_empty() && left_anchor {
        return segment.start_byte < request.start_byte && request.start_byte <= segment.end_byte;
    }
    segment.start_byte <= request.start_byte
        && request.start_byte < segment.end_byte
        && request.end_byte <= segment.end_byte
}
fn append_choices(
    alternatives: &mut Vec<Vec<Contribution>>,
    choices: &[Vec<Contribution>],
    budget: &mut MapBudget<'_>,
) -> Result<(), SourceError> {
    let count = alternatives
        .len()
        .checked_mul(choices.len())
        .ok_or(SourceError::LimitExceeded("alternatives"))?;
    MapBudget::charge(
        &mut budget.usage.branches,
        count,
        budget.limits.branches,
        "alternatives",
    )?;
    budget.allocate(count * std::mem::size_of::<Vec<Contribution>>())?;
    let mut expanded = Vec::with_capacity(count);
    for previous in alternatives.iter() {
        for choice in choices {
            budget.check()?;
            let size = previous
                .len()
                .checked_add(choice.len())
                .ok_or(SourceError::LimitExceeded("contributors"))?;
            for _ in 0..size {
                budget.contributor()?;
            }
            budget.allocate(size * std::mem::size_of::<Contribution>())?;
            let mut strings = 0usize;
            for contribution in previous.iter().chain(choice) {
                budget.check()?;
                if let Contribution::Source(origin) = contribution {
                    strings = strings
                        .checked_add(origin.source_id.len())
                        .ok_or(SourceError::LimitExceeded("contributors"))?;
                }
            }
            budget.allocate(strings)?;
            expanded.push(previous.iter().chain(choice).cloned().collect());
        }
    }
    *alternatives = expanded;
    Ok(())
}
