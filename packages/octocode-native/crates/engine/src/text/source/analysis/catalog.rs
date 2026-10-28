use super as domain;
use super::*;

pub struct SourceCatalog<'a> {
    sources: Index<'a, domain::SourceSnapshot>,
    representations: Index<'a, domain::TextRepresentation>,
    units: Index<'a, domain::AnalysisUnit>,
}

struct Index<'a, T>(Vec<(&'a str, &'a T)>);
impl<T> Index<'_, T> {
    fn get(&self, key: &str) -> Option<&T> {
        self.0
            .binary_search_by_key(&key, |(id, _)| *id)
            .ok()
            .map(|index| self.0[index].1)
    }
}

fn unique<'a, T>(
    rows: &'a [T],
    id: impl Fn(&'a T) -> &'a str,
    budget: &mut MapBudget<'_>,
) -> Result<Index<'a, T>, SourceError> {
    let mut output = Vec::new();
    budget.reserve(&mut output, rows.len())?;
    for row in rows {
        budget.check()?;
        let id = id(row);
        if id.is_empty() {
            return Err(invalid(
                "source descriptors contain an empty or duplicate identity",
            ));
        }
        output.push((id, row));
    }
    output.sort_unstable_by_key(|(id, _)| *id);
    if output.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(invalid(
            "source descriptors contain an empty or duplicate identity",
        ));
    }
    Ok(Index(output))
}

impl<'a> SourceCatalog<'a> {
    pub fn new(
        value: &'a SourceDescriptors,
        budget: &mut MapBudget<'_>,
    ) -> Result<Self, SourceError> {
        let index = Self {
            sources: unique(&value.snapshots, |row| &row.id, budget)?,
            representations: unique(&value.representations, |row| &row.id, budget)?,
            units: unique(&value.units, |row| &row.id, budget)?,
        };
        validate_catalog(value, &index, budget)?;
        Ok(index)
    }
}

fn validate_catalog(
    value: &SourceDescriptors,
    index: &SourceCatalog<'_>,
    budget: &mut MapBudget<'_>,
) -> Result<(), SourceError> {
    let digest = |value: &str| {
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    for source in &value.snapshots {
        budget.check()?;
        let valid_bom = match source.encoding {
            domain::Encoding::Utf8 => matches!(source.bom_bytes, 0 | 3),
            domain::Encoding::Utf16Le | domain::Encoding::Utf16Be => source.bom_bytes == 2,
        };
        if source.path.is_empty()
            || !digest(&source.digest)
            || !valid_bom
            || source.bom_bytes > source.byte_length
            || (source.encoding != Encoding::Utf8 && source.byte_length % 2 != 0)
        {
            return Err(invalid(
                "source snapshot has invalid identity, digest, encoding or byte length",
            ));
        }
    }
    for representation in &value.representations {
        budget.check()?;
        let mut seen = Vec::new();
        budget.reserve(&mut seen, representation.source_ids.len())?;
        if !digest(&representation.source_digest)
            || representation.producer.id.is_empty()
            || representation.producer.version.is_empty()
        {
            return Err(invalid(
                "source representation has invalid or dangling source descriptors",
            ));
        }
        for id in &representation.source_ids {
            budget.check()?;
            if index.sources.get(id.as_str()).is_none() {
                return Err(invalid(
                    "source representation has invalid or dangling source descriptors",
                ));
            }
            seen.push(id.as_str());
        }
        seen.sort_unstable();
        if seen.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(invalid(
                "source representation has duplicate source descriptors",
            ));
        }
    }
    for unit in &value.units {
        budget.check()?;
        let representation = index
            .representations
            .get(unit.representation_id.as_str())
            .ok_or_else(|| invalid("analysis unit has no representation descriptor"))?;
        let mut end = 0;
        if unit.adapter.id.is_empty() || unit.adapter.version.is_empty() || unit.regions.is_empty()
        {
            return Err(invalid("analysis unit has no adapter or analyzed region"));
        }
        for region in &unit.regions {
            budget.check()?;
            if region.start_byte > region.end_byte
                || region.end_byte > representation.byte_length
                || region.start_byte < end
            {
                return Err(invalid(
                    "analysis unit has overlapping, reversed or out-of-range regions",
                ));
            }
            end = region.end_byte;
        }
    }
    Ok(())
}

impl SourceCatalog<'_> {
    pub fn validate_projection(
        &self,
        origin: &OriginProjection,
        range: ByteRange,
        budget: &mut MapBudget<'_>,
    ) -> Result<(), SourceError> {
        validate_origin_index(origin, range, self, budget)
    }
}

fn validate_origin_index(
    origin: &OriginProjection,
    range: domain::ByteRange,
    index: &SourceCatalog<'_>,
    budget: &mut MapBudget<'_>,
) -> Result<(), SourceError> {
    let unit = index
        .units
        .get(origin.analysis_unit_id.as_str())
        .ok_or_else(|| invalid("source origin has no analysis unit descriptor"))?;
    let representation = index
        .representations
        .get(unit.representation_id.as_str())
        .ok_or_else(|| invalid("source origin unit has no representation descriptor"))?;
    if range.start_byte > range.end_byte
        || range.end_byte > representation.byte_length
        || origin.segments.is_empty()
    {
        return Err(invalid(
            "source origin has an invalid paired range or no segments",
        ));
    }
    let valid_original = |original: &domain::OriginalRange,
                          segment: domain::ByteRange,
                          copied: bool,
                          budget: &mut MapBudget<'_>|
     -> Result<bool, SourceError> {
        let Some(source) = index.sources.get(original.source_id.as_str()) else {
            return Ok(false);
        };
        let mut member = false;
        for id in &representation.source_ids {
            budget.check()?;
            if id == &original.source_id {
                member = true;
                break;
            }
        }
        Ok(member
            && original.raw_range.start_byte <= original.raw_range.end_byte
            && original.raw_range.start_byte >= source.bom_bytes
            && original.raw_range.end_byte <= source.byte_length
            && (!copied
                || (source.encoding == Encoding::Utf8
                    && original.raw_range.len() == segment.len()))
            && (source.encoding == Encoding::Utf8
                || (original.raw_range.start_byte.is_multiple_of(2)
                    && original.raw_range.end_byte.is_multiple_of(2))))
    };
    let mut end = range.start_byte;
    for segment in &origin.segments {
        budget.check()?;
        let span = segment.source_range;
        if span.start_byte != end
            || span.end_byte < span.start_byte
            || span.end_byte > range.end_byte
            || (span.is_empty() && (!range.is_empty() || origin.segments.len() != 1))
            || segment.branches.is_empty()
        {
            return Err(invalid(
                "source origin segments do not partition the paired range",
            ));
        }
        let mut included = false;
        let mut crosses_region = false;
        for region in &unit.regions {
            budget.check()?;
            included |= if !span.is_empty() {
                region.start_byte <= span.start_byte && span.end_byte <= region.end_byte
            } else if span.start_byte == representation.byte_length && span.start_byte != 0 {
                region.start_byte < span.start_byte && span.start_byte <= region.end_byte
            } else {
                region.start_byte <= span.start_byte
                    && (span.start_byte < region.end_byte || representation.byte_length == 0)
            };
            crosses_region |= !span.is_empty()
                && region.start_byte < span.end_byte
                && span.start_byte < region.end_byte;
        }
        let crosses_region = crosses_region && !included;
        if crosses_region
            || included != matches!(segment.analysis, domain::AnalysisMembership::Included)
        {
            return Err(invalid(
                "source origin analysis membership disagrees with its unit",
            ));
        }
        for branch in &segment.branches {
            budget.check()?;
            let valid = match branch {
                domain::OriginBranch::Copied(value) => valid_original(value, span, true, budget)?,
                domain::OriginBranch::Transcoded(value) => {
                    valid_original(value, span, false, budget)?
                        && index
                            .sources
                            .get(value.source_id.as_str())
                            .is_some_and(|source| source.encoding != Encoding::Utf8)
                }
                domain::OriginBranch::Unmapped(_) => true,
                domain::OriginBranch::Derived(values) => {
                    let mut valid = !values.is_empty();
                    for value in values {
                        budget.check()?;
                        valid &= match value {
                            domain::Contribution::Source(value) => {
                                valid_original(value, span, false, budget)?
                            }
                            domain::Contribution::Unmapped(_) => true,
                        };
                    }
                    valid
                }
            };
            if !valid {
                return Err(invalid(
                    "source origin contains an invalid or dangling branch",
                ));
            }
        }
        end = span.end_byte;
    }
    if end != range.end_byte {
        return Err(invalid("source origin omits part of its paired range"));
    }
    Ok(())
}

fn invalid(message: &'static str) -> SourceError {
    SourceError::InvalidMap(message)
}
