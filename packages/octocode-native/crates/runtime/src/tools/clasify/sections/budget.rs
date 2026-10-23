//! The call-wide admission budget every sections row shares.
use super::capture;
use crate::execution::SelectionLimits;
use std::sync::Mutex;
use std::time::Instant;

pub(in crate::tools::clasify) struct Budget {
    pub(super) limits: SelectionLimits,
    admission: Mutex<Admission>,
    pub(super) captures: capture::Pool,
}

#[derive(Default)]
struct Admission {
    units: usize,
    chars: usize,
    deadline: Option<Instant>,
}

impl Budget {
    pub(in crate::tools::clasify) fn new(limits: SelectionLimits) -> Self {
        Self {
            limits,
            admission: Mutex::new(Admission::default()),
            captures: capture::Pool::default(),
        }
    }

    pub(super) fn deadline(&self, outer: Instant) -> Instant {
        let mut admission = self
            .admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *admission
            .deadline
            .get_or_insert_with(|| outer.min(Instant::now() + self.limits.timeout))
    }

    pub(super) fn has_admissions(&self) -> bool {
        self.admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .units
            > 0
    }

    pub(super) fn admit(&self, chars: usize) -> bool {
        let mut admission = self
            .admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if admission.units >= self.limits.max_candidates
            || admission.chars.saturating_add(chars) > self.limits.max_chars
            || admission
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return false;
        }
        admission.units += 1;
        admission.chars += chars;
        true
    }
}
