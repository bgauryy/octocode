use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub struct MapLimits {
    pub depth: usize,
    pub representations: usize,
    pub segments: usize,
    pub branches: usize,
    pub contributors: usize,
    pub bytes: usize,
    pub work: usize,
}
impl Default for MapLimits {
    fn default() -> Self {
        Self {
            depth: 32,
            representations: 1024,
            segments: 65_536,
            branches: 262_144,
            contributors: 524_288,
            bytes: 32 << 20,
            work: 4_000_000,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapUsage {
    pub representations: usize,
    pub segments: usize,
    pub branches: usize,
    pub contributors: usize,
    pub bytes: usize,
    pub work: usize,
    pub peak_temporary_bytes: usize,
}
pub struct MapBudget<'a> {
    pub limits: MapLimits,
    pub usage: MapUsage,
    deadline: Instant,
    cancelled: &'a (dyn Fn() -> bool + Sync),
}
impl<'a> MapBudget<'a> {
    pub fn new(
        limits: MapLimits,
        deadline: Instant,
        cancelled: &'a (dyn Fn() -> bool + Sync),
    ) -> Self {
        Self {
            limits,
            usage: MapUsage::default(),
            deadline,
            cancelled,
        }
    }
    pub fn check(&mut self) -> Result<(), SourceError> {
        if (self.cancelled)() {
            return Err(SourceError::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(SourceError::Deadline);
        }
        Self::charge(&mut self.usage.work, 1, self.limits.work, "work")
    }
    pub(super) fn temporary_size(
        &mut self,
        price: impl FnOnce(&mut Self) -> Result<usize, SourceError>,
    ) -> Result<usize, SourceError> {
        self.check()?;
        let limits = MapLimits {
            bytes: self.limits.bytes.saturating_sub(self.usage.bytes),
            work: self.limits.work.saturating_sub(self.usage.work),
            ..self.limits
        };
        let mut temporary = Self::new(limits, self.deadline, self.cancelled);
        let result = price(&mut temporary);
        self.usage.work = self.usage.work.saturating_add(temporary.usage.work);
        self.usage.peak_temporary_bytes = self
            .usage
            .peak_temporary_bytes
            .max(temporary.usage.bytes.min(limits.bytes));
        result
    }

    pub(super) fn depth(&mut self, depth: usize) -> Result<(), SourceError> {
        self.check()?;
        if depth > self.limits.depth {
            Err(SourceError::LimitExceeded("depth"))
        } else {
            Ok(())
        }
    }
    pub(super) fn charge(
        value: &mut usize,
        add: usize,
        max: usize,
        name: &'static str,
    ) -> Result<(), SourceError> {
        *value = value
            .checked_add(add)
            .ok_or(SourceError::LimitExceeded(name))?;
        if *value > max {
            Err(SourceError::LimitExceeded(name))
        } else {
            Ok(())
        }
    }
    pub(crate) fn reserve<T>(
        &mut self,
        values: &mut Vec<T>,
        additional: usize,
    ) -> Result<(), SourceError> {
        self.check()?;
        let needed = values
            .len()
            .checked_add(additional)
            .ok_or(SourceError::LimitExceeded("allocation"))?;
        if needed > values.capacity() {
            let target = needed.max(values.capacity().saturating_mul(2)).max(4);
            let old = values.capacity();
            self.allocate(
                target
                    .checked_sub(old)
                    .and_then(|n| n.checked_mul(std::mem::size_of::<T>()))
                    .ok_or(SourceError::LimitExceeded("allocation"))?,
            )?;
            values
                .try_reserve_exact(target - values.len())
                .map_err(|_| SourceError::LimitExceeded("allocation"))?;
            self.allocate(
                values
                    .capacity()
                    .saturating_sub(target)
                    .saturating_mul(std::mem::size_of::<T>()),
            )?;
        }
        Ok(())
    }
    pub fn allocate(&mut self, bytes: usize) -> Result<(), SourceError> {
        Self::charge(&mut self.usage.bytes, bytes, self.limits.bytes, "bytes")
    }
    pub(super) fn segment(&mut self) -> Result<(), SourceError> {
        self.check()?;
        Self::charge(
            &mut self.usage.segments,
            1,
            self.limits.segments,
            "segments",
        )
    }
    pub(super) fn branch(&mut self) -> Result<(), SourceError> {
        Self::charge(
            &mut self.usage.branches,
            1,
            self.limits.branches,
            "branches",
        )
    }
    pub(super) fn contributor(&mut self) -> Result<(), SourceError> {
        Self::charge(
            &mut self.usage.contributors,
            1,
            self.limits.contributors,
            "contributors",
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceError {
    Refused(super::super::Refusal),
    InvalidMap(&'static str),
    Cancelled,
    Deadline,
    LimitExceeded(&'static str),
}
impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source correspondence: {self:?}")
    }
}
impl std::error::Error for SourceError {}
