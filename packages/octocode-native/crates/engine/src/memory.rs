//! The bytes every in-process store holds together.
use octocode_io::memo::MemoryBudget;
use std::time::Duration;

/// The bytes every in-process store holds together: each tool's page memo, the
/// memory tier of every disk store, and what the LSP discovery keeps. Past it
/// the least recently used entries of all of them go first, and a value larger
/// than all of it is not kept.
pub static MEMORY: MemoryBudget = MemoryBudget::new(memory_budget);

/// How long a kept entry stays once nothing reads it, for a memo whose value
/// can be worked out again: a read restarts it.
pub const IDLE: Duration = Duration::from_secs(600);

/// The budget when the host's memory cannot be read. A known host always
/// uses its proportional share, even when that is smaller than this default.
pub const DEFAULT_MEMORY_BUDGET: usize = 512 * 1024 * 1024;
/// The budget's ceiling: retained continuation state is a convenience, so a
/// large host keeps most of its memory for language servers and its users.
pub const MAX_MEMORY_BUDGET: usize = 2 * 1024 * 1024 * 1024;

fn memory_budget() -> usize {
    memory_budget_for(crate::host::memory_mb())
}

/// An eighth of `host_mb` MiB, at most [`MAX_MEMORY_BUDGET`];
/// [`DEFAULT_MEMORY_BUDGET`] when the host's memory is unknown.
#[must_use]
pub fn memory_budget_for(host_mb: Option<u64>) -> usize {
    host_mb.map_or(DEFAULT_MEMORY_BUDGET, |mb| {
        usize::try_from(mb.saturating_mul(1024 * 1024) / 8)
            .unwrap_or(usize::MAX)
            .min(MAX_MEMORY_BUDGET)
    })
}

/// The bytes the workers of one process hold at once for work that needs a
/// whole file in memory (a judge's text, a line longer than the line
/// buffer): what is admitted never passes the documented memory budget,
/// however many workers run and however large their files are.
pub static WHOLE_FILES: ByteAdmission = ByteAdmission::new(whole_files_limit);

fn whole_files_limit() -> usize {
    #[cfg(test)]
    if let Some(limit) = test_limit::get() {
        return limit;
    }
    MEMORY.limit()
}

/// A gate that admits work by the bytes it will hold. A request waits while
/// the bytes admitted plus its own would pass the limit; a request larger than
/// the limit is clamped to it, so that work runs alone and is never refused.
/// Requests that wait are served in arrival order, so a large one is not
/// starved by small ones that keep arriving.
pub struct ByteAdmission {
    limit: fn() -> usize,
    state: std::sync::Mutex<Admitted>,
    freed: std::sync::Condvar,
    #[cfg(test)]
    peak: std::sync::atomic::AtomicUsize,
}

struct Admitted {
    used: usize,
    /// The waiting requests, oldest first, by the number each was given.
    queue: std::collections::VecDeque<u64>,
    next: u64,
    /// Bytes admitted since freed memory was last given back to the system.
    since_relief: usize,
}

/// Bytes admitted; they are released when it is dropped.
pub struct Ticket<'a> {
    gate: &'a ByteAdmission,
    bytes: usize,
}

/// How often a waiting request looks at whether its search was stopped.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        reason = "a polling granularity for noticing a cancel, not a work budget: the request's own deadline still bounds the wait"
    )
)]
const WAIT_SLICE: Duration = Duration::from_millis(25);

impl ByteAdmission {
    pub const fn new(limit: fn() -> usize) -> Self {
        Self {
            limit,
            state: std::sync::Mutex::new(Admitted {
                used: 0,
                queue: std::collections::VecDeque::new(),
                next: 0,
                since_relief: 0,
            }),
            freed: std::sync::Condvar::new(),
            #[cfg(test)]
            peak: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Wait until `bytes` may be held, then admit them. `None` when
    /// `halted()` turned true while waiting: the search was stopped, so
    /// nothing is read.
    pub fn acquire(&self, bytes: usize, halted: &dyn Fn() -> bool) -> Option<Ticket<'_>> {
        let limit = self.limit();
        let bytes = bytes.min(limit);
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.queue.is_empty() && state.used.saturating_add(bytes) <= limit {
            return Some(self.admit(&mut state, bytes));
        }
        let turn = state.next;
        state.next += 1;
        state.queue.push_back(turn);
        loop {
            if state.queue.front() == Some(&turn) && state.used.saturating_add(bytes) <= limit {
                state.queue.pop_front();
                let ticket = self.admit(&mut state, bytes);
                // The next in line may fit beside this one.
                self.freed.notify_all();
                return Some(ticket);
            }
            if halted() {
                state.queue.retain(|queued| *queued != turn);
                self.freed.notify_all();
                return None;
            }
            state = self
                .freed
                .wait_timeout(state, WAIT_SLICE)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    /// The most bytes admitted at once.
    #[must_use]
    pub fn limit(&self) -> usize {
        (self.limit)().max(1)
    }

    fn admit(&self, state: &mut Admitted, bytes: usize) -> Ticket<'_> {
        state.used += bytes;
        state.since_relief = state.since_relief.saturating_add(bytes);
        #[cfg(test)]
        self.peak
            .fetch_max(state.used, std::sync::atomic::Ordering::Relaxed);
        Ticket { gate: self, bytes }
    }

    /// The most bytes admitted at once since the last reset.
    #[cfg(test)]
    pub(crate) fn peak(&self) -> usize {
        self.peak.load(std::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(crate) fn reset_peak(&self) {
        self.peak.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Ticket<'_> {
    /// Keep only `bytes` of what was admitted (no more than it holds): the
    /// rest is released to the requests that wait.
    pub fn shrink_to(&mut self, bytes: usize) {
        let release = self.bytes.saturating_sub(bytes);
        if release == 0 {
            return;
        }
        self.bytes -= release;
        let mut state = self
            .gate
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.used = state.used.saturating_sub(release);
        drop(state);
        self.gate.freed.notify_all();
    }
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        let mut state = self
            .gate
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.used = state.used.saturating_sub(self.bytes);
        // The system keeps the large regions a worker freed (they stay in the
        // process's footprint until idle exit), so once enough was held it
        // takes them back: when the gate drains, or at a fixed share of the
        // budget while workers keep it busy.
        let relieve = (state.used == 0 && state.since_relief >= RELIEF_AT_DRAIN)
            || state.since_relief >= self.gate.limit() / 8;
        if relieve {
            state.since_relief = 0;
        }
        drop(state);
        self.gate.freed.notify_all();
        if relieve {
            release_freed_memory();
        }
    }
}

/// Bytes admitted since the last relief that make the gate's drain worth one.
const RELIEF_AT_DRAIN: usize = 16 * 1024 * 1024;

/// Gives the system the large regions this process freed but its allocator
/// still holds. A no-op where the allocator returns them itself.
pub fn release_freed_memory() {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        // SAFETY: a null zone asks every zone to release what it can; the call
        // takes no pointer and frees only memory the allocator already holds free.
        unsafe {
            malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
        }
    }
}

/// Bytes held for one whole-file read, taken from the system apart from the
/// allocator: dropping them returns the pages at once, where the allocator
/// keeps a freed large block (dirty, in the process's footprint) until idle exit.
pub struct Region {
    #[cfg(unix)]
    ptr: *mut u8,
    #[cfg(not(unix))]
    heap: Vec<u8>,
    len: usize,
}

// SAFETY: the region is plain bytes owned by this value alone, with no
// interior mutability: moving it between threads is sound.
#[cfg(unix)]
unsafe impl Send for Region {}
// SAFETY: shared access only reads the bytes (`as_slice`); writes need `&mut self`.
#[cfg(unix)]
unsafe impl Sync for Region {}

impl Region {
    /// A zeroed region of `len` bytes.
    pub fn new(len: usize) -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            if len == 0 {
                return Ok(Self {
                    ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(),
                    len,
                });
            }
            // SAFETY: an anonymous private mapping takes no file or address.
            let ptr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANON,
                    -1,
                    0,
                )
            };
            if ptr == libc::MAP_FAILED {
                return Err(std::io::Error::last_os_error());
            }
            Ok(Self {
                ptr: ptr.cast(),
                len,
            })
        }
        #[cfg(not(unix))]
        {
            Ok(Self {
                heap: vec![0; len],
                len,
            })
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        #[cfg(unix)]
        {
            // SAFETY: `ptr` maps `len` initialized (zeroed) bytes this value owns.
            unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
        }
        #[cfg(not(unix))]
        {
            &self.heap
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        #[cfg(unix)]
        {
            // SAFETY: as `as_slice`, and `&mut self` is the only access.
            unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
        }
        #[cfg(not(unix))]
        {
            &mut self.heap
        }
    }
}

#[cfg(unix)]
impl Drop for Region {
    fn drop(&mut self) {
        if self.len > 0 {
            // SAFETY: unmaps exactly the mapping `new` made.
            unsafe {
                libc::munmap(self.ptr.cast(), self.len);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod test_limit;
