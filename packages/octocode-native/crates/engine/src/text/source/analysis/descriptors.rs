use super::*;

impl BoundSource<'_> {
    pub fn descriptors(
        &self,
        projections: &[OriginProjection],
        budget: &mut MapBudget<'_>,
    ) -> Result<SourceDescriptors, SourceError> {
        budget.allocate(
            projections
                .len()
                .saturating_mul(std::mem::size_of::<UnitHandle>()),
        )?;
        let handles = projections
            .iter()
            .map(|projection| {
                self.units
                    .iter()
                    .position(|u| u.id == projection.analysis_unit_id)
                    .ok_or(SourceError::InvalidMap("unknown projection unit"))
                    .and_then(|i| {
                        u32::try_from(i)
                            .map(UnitHandle)
                            .map_err(|_| SourceError::LimitExceeded("analysis units"))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.descriptors_for_units(&handles, budget)
    }
    pub fn descriptors_for_units(
        &self,
        handles: &[UnitHandle],
        budget: &mut MapBudget<'_>,
    ) -> Result<SourceDescriptors, SourceError> {
        let mut result = SourceDescriptors::default();
        for handle in handles {
            budget.check()?;
            let unit = self
                .units
                .get(handle.0 as usize)
                .ok_or(SourceError::InvalidMap("unknown projection unit"))?;
            let representation = self.representation_for_unit(*handle)?;
            merge_by_id(
                &mut result.units,
                std::slice::from_ref(unit),
                |v| &v.id,
                budget,
            )?;
            merge_by_id(
                &mut result.representations,
                std::slice::from_ref(representation),
                |v| &v.id,
                budget,
            )?;
            for id in &representation.source_ids {
                let snapshot = self
                    .sources
                    .iter()
                    .filter_map(|s| s.snapshot.as_ref())
                    .find(|s| &s.id == id)
                    .ok_or(SourceError::InvalidMap("unknown projection source"))?;
                merge_by_id(
                    &mut result.snapshots,
                    std::slice::from_ref(snapshot),
                    |v| &v.id,
                    budget,
                )?;
            }
        }
        Ok(result)
    }

    pub fn representation_for_unit(
        &self,
        unit: UnitHandle,
    ) -> Result<&TextRepresentation, SourceError> {
        let template = self
            .templates
            .units
            .get(unit.0 as usize)
            .ok_or(SourceError::InvalidMap("unknown unit"))?;
        self.representations
            .get(template.representation.0 as usize)
            .ok_or(SourceError::InvalidMap("unknown representation"))
    }
}

impl SourceDescriptors {
    pub fn merge(&mut self, other: &Self, budget: &mut MapBudget<'_>) -> Result<(), SourceError> {
        merge_by_id(&mut self.snapshots, &other.snapshots, |v| &v.id, budget)?;
        merge_by_id(
            &mut self.representations,
            &other.representations,
            |v| &v.id,
            budget,
        )?;
        merge_by_id(&mut self.units, &other.units, |v| &v.id, budget)
    }
    pub fn closure(
        &self,
        projections: &[OriginProjection],
        budget: &mut MapBudget<'_>,
    ) -> Result<Self, SourceError> {
        let catalog = SourceCatalog::new(self, budget)?;
        let mut result = Self::default();
        for projection in projections {
            let first = projection
                .segments
                .first()
                .ok_or(SourceError::InvalidMap("empty projection"))?;
            let last = projection
                .segments
                .last()
                .ok_or(SourceError::InvalidMap("empty projection"))?;
            catalog.validate_projection(
                projection,
                ByteRange::new(first.source_range.start_byte, last.source_range.end_byte),
                budget,
            )?;
            budget.check()?;
            let unit = self
                .units
                .iter()
                .find(|u| u.id == projection.analysis_unit_id)
                .ok_or(SourceError::InvalidMap("missing unit descriptor"))?;
            let representation = self
                .representations
                .iter()
                .find(|r| r.id == unit.representation_id)
                .ok_or(SourceError::InvalidMap("missing representation descriptor"))?;
            merge_by_id(
                &mut result.units,
                std::slice::from_ref(unit),
                |v| &v.id,
                budget,
            )?;
            merge_by_id(
                &mut result.representations,
                std::slice::from_ref(representation),
                |v| &v.id,
                budget,
            )?;
            for id in &representation.source_ids {
                let snapshot = self
                    .snapshots
                    .iter()
                    .find(|s| &s.id == id)
                    .ok_or(SourceError::InvalidMap("missing snapshot descriptor"))?;
                merge_by_id(
                    &mut result.snapshots,
                    std::slice::from_ref(snapshot),
                    |v| &v.id,
                    budget,
                )?;
            }
        }
        Ok(result)
    }
    pub fn owned_bytes(&self) -> usize {
        self.snapshots.capacity() * std::mem::size_of::<SourceSnapshot>()
            + self
                .snapshots
                .iter()
                .map(|s| s.id.capacity() + s.path.capacity() + s.digest.capacity())
                .sum::<usize>()
            + self.representations.capacity() * std::mem::size_of::<TextRepresentation>()
            + self
                .representations
                .iter()
                .map(|r| {
                    r.id.capacity()
                        + r.source_digest.capacity()
                        + r.producer.id.capacity()
                        + r.producer.version.capacity()
                        + r.source_ids.capacity() * std::mem::size_of::<String>()
                        + r.source_ids.iter().map(String::capacity).sum::<usize>()
                })
                .sum::<usize>()
            + self.units.capacity() * std::mem::size_of::<AnalysisUnit>()
            + self
                .units
                .iter()
                .map(|u| {
                    u.id.capacity()
                        + u.representation_id.capacity()
                        + u.adapter.id.capacity()
                        + u.adapter.version.capacity()
                        + u.regions.capacity() * std::mem::size_of::<ByteRange>()
                })
                .sum::<usize>()
    }
}
fn merge_by_id<T: DescriptorAllocation>(
    target: &mut Vec<T>,
    source: &[T],
    id: impl Fn(&T) -> &str,
    budget: &mut MapBudget<'_>,
) -> Result<(), SourceError> {
    for value in source {
        budget.check()?;
        if let Some(old) = target.iter().find(|old| id(old) == id(value)) {
            if old != value {
                return Err(SourceError::InvalidMap("conflicting descriptor identity"));
            }
        } else {
            budget.reserve(target, 1)?;
            budget.allocate(value.allocated_bytes())?;
            target.push(value.clone());
        }
    }
    Ok(())
}

trait DescriptorAllocation: Clone + PartialEq {
    fn allocated_bytes(&self) -> usize;
}
impl DescriptorAllocation for SourceSnapshot {
    fn allocated_bytes(&self) -> usize {
        self.id.capacity() + self.path.capacity() + self.digest.capacity()
    }
}
impl DescriptorAllocation for TextRepresentation {
    fn allocated_bytes(&self) -> usize {
        self.id.capacity()
            + self.source_digest.capacity()
            + self.producer.id.capacity()
            + self.producer.version.capacity()
            + self.source_ids.capacity() * std::mem::size_of::<String>()
            + self.source_ids.iter().map(String::capacity).sum::<usize>()
    }
}
impl DescriptorAllocation for AnalysisUnit {
    fn allocated_bytes(&self) -> usize {
        self.id.capacity()
            + self.representation_id.capacity()
            + self.adapter.id.capacity()
            + self.adapter.version.capacity()
            + self.regions.capacity() * std::mem::size_of::<ByteRange>()
    }
}
