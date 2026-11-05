//! The call-wide admission budget every sections row shares.
use super::capture;
use crate::execution::SelectionLimits;
use std::sync::Mutex;

pub(in crate::tools::clasify) struct Budget {
    pub(super) limits: SelectionLimits,
    admission: Mutex<Admission>,
    pub(super) captures: capture::Pool,
}

#[derive(Default)]
struct Admission {
    units: usize,
    chars: usize,
}

impl Budget {
    pub(in crate::tools::clasify) fn new(limits: SelectionLimits) -> Self {
        Self {
            limits,
            admission: Mutex::new(Admission::default()),
            captures: capture::Pool::default(),
        }
    }

    pub(super) fn has_admissions(&self) -> bool {
        self.admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .units
            > 0
    }

    /// Whether this call's page takes `chars` more: its first unit always
    /// runs, whatever its size, and the next page takes what this one does
    /// not. A runtime with no next page (`whole`) takes everything.
    pub(super) fn admit(&self, chars: usize, whole: bool) -> bool {
        let mut admission = self
            .admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !whole
            && admission.units > 0
            && admission.chars.saturating_add(chars) > self.limits.max_chars
        {
            return false;
        }
        admission.units += 1;
        admission.chars = admission.chars.saturating_add(chars);
        true
    }
}
