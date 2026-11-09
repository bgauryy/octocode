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
    /// What `bytes` stands for, named when a source passes it: the caller's
    /// budget it was sized from.
    pub bytes_budget: &'static str,
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
            bytes_budget: "per-file source correspondence budget",
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
/// Memory shared beyond one budget: every byte a [`MapBudget`] allocates
/// is also drawn here, so the files of one scan in flight together never
/// hold more than their caller's whole budget.
pub trait Draw: Sync {
    /// Takes `bytes`, or refuses with [`SourceError::LimitExceeded`]
    /// (`"bytes"`).
    fn draw(&self, bytes: usize) -> Result<(), SourceError>;
    /// Returns `bytes` an earlier draw took and no longer holds.
    fn release(&self, bytes: usize);
}

pub struct MapBudget<'a> {
    pub limits: MapLimits,
    pub usage: MapUsage,
    deadline: Instant,
    cancelled: &'a (dyn Fn() -> bool + Sync),
    shared: Option<&'a dyn Draw>,
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
            shared: None,
        }
    }
    /// This budget, also drawing every allocation from `shared`.
    #[must_use]
    pub fn drawing(mut self, shared: Option<&'a dyn Draw>) -> Self {
        self.shared = shared;
        self
    }
    /// A budget with these limits, deadline, cancellation and shared draw,
    /// none of this one's usage.
    #[must_use]
    pub fn fresh(&self) -> Self {
        Self::new(self.limits, self.deadline, self.cancelled).drawing(self.shared)
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
        let mut temporary = Self::new(limits, self.deadline, self.cancelled).drawing(self.shared);
        let result = price(&mut temporary);
        // What the temporary held is freed with it.
        if let Some(shared) = self.shared {
            shared.release(temporary.usage.bytes);
        }
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
    /// What passing the limit `name` (a [`SourceError::LimitExceeded`] of
    /// this budget) means: the limit, its ceiling and what was used when
    /// it stopped.
    #[must_use]
    pub fn exceeded(&self, name: &str) -> String {
        let (limits, usage) = (&self.limits, &self.usage);
        let used = match name {
            "bytes" => {
                return format!(
                    "needs more than the {} of {} bytes ({} charged when it stopped)",
                    limits.bytes_budget, limits.bytes, usage.bytes
                );
            }
            "work" => Some((limits.work, usage.work)),
            "representations" => Some((limits.representations, usage.representations)),
            "segments" => Some((limits.segments, usage.segments)),
            "branches" => Some((limits.branches, usage.branches)),
            "contributors" => Some((limits.contributors, usage.contributors)),
            "depth" => return format!("passes its depth limit of {}", limits.depth),
            "alternatives" => Some((limits.branches, usage.branches)),
            "allocation" | "boundaries" => {
                return format!(
                    "needs an allocation this platform cannot size or make ({name}; {} bytes charged when it stopped)",
                    usage.bytes
                );
            }
            _ => None,
        };
        match used {
            Some((limit, used)) => {
                format!("passes its {name} limit of {limit} ({used} used when it stopped)")
            }
            None => format!("passes its {name} bound"),
        }
    }
    pub fn allocate(&mut self, bytes: usize) -> Result<(), SourceError> {
        Self::charge(&mut self.usage.bytes, bytes, self.limits.bytes, "bytes")?;
        match self.shared {
            Some(shared) => shared.draw(bytes).inspect_err(|_| {
                // Not taken: this budget does not hold it either.
                self.usage.bytes -= bytes;
            }),
            None => Ok(()),
        }
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
        match self {
            Self::Refused(refusal) => {
                write!(f, "source correspondence: source refused ({refusal:?})")
            }
            Self::InvalidMap(what) => write!(f, "source correspondence: invalid map: {what}"),
            Self::Cancelled => f.write_str("source correspondence: cancelled"),
            Self::Deadline => f.write_str("source correspondence: deadline passed"),
            Self::LimitExceeded(what) => write!(f, "source correspondence: {what} exceeded"),
        }
    }
}
impl std::error::Error for SourceError {}
