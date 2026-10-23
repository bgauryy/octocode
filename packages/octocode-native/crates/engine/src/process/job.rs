//! The Windows kill-on-close job that contains a child (a no-op handle
//! elsewhere).

/// Holds a kill-on-close Windows job, optionally with a committed-memory cap.
/// Other platforms use process-group/tree teardown and need no job handle.
#[cfg(not(windows))]
#[derive(Debug)]
pub struct Job;

#[cfg(windows)]
#[derive(Debug)]
pub struct Job(Option<windows_sys::Win32::Foundation::HANDLE>);

// SAFETY: this owned kernel handle may be used and closed from any thread.
#[cfg(windows)]
unsafe impl Send for Job {}

impl Job {
    /// Contain a synchronous child and optionally cap its job's memory.
    pub fn attach(
        child: &std::process::Child,
        cap_bytes: Option<u64>,
    ) -> crate::error::Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            Self::from_handle(Some(child.as_raw_handle()), cap_bytes)
        }
        #[cfg(not(windows))]
        {
            let _ = (child, cap_bytes);
            Ok(Self)
        }
    }

    /// Contain a Tokio child and optionally cap its job's memory.
    pub fn attach_async(
        child: &tokio::process::Child,
        cap_bytes: Option<u64>,
    ) -> crate::error::Result<Self> {
        #[cfg(windows)]
        {
            Self::from_handle(child.raw_handle(), cap_bytes)
        }
        #[cfg(not(windows))]
        {
            let _ = (child, cap_bytes);
            Ok(Self)
        }
    }

    /// Close the job before waiting for pipe readers, terminating stragglers.
    pub fn terminate(&mut self) {
        #[cfg(windows)]
        if let Some(job) = self.0.take() {
            // SAFETY: this guard owns the handle and closes it exactly once.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
        }
    }

    #[cfg(windows)]
    fn from_handle(
        raw: Option<std::os::windows::io::RawHandle>,
        cap_bytes: Option<u64>,
    ) -> crate::error::Result<Self> {
        use std::mem::{size_of, zeroed};
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_JOB_MEMORY,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectExtendedLimitInformation, SetInformationJobObject,
        };
        let Some(raw) = raw else {
            return Ok(Self(None));
        };
        let failure = |what: &str| {
            crate::error::Error::new(format!(
                "Failed to {what} for the process guard: {}",
                std::io::Error::last_os_error()
            ))
        };
        // SAFETY: null security/name creates an unnamed job owned here.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(failure("create a Windows job object"));
        }
        // SAFETY: zero is the documented baseline for this plain structure.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if let Some(cap_bytes) = cap_bytes {
            info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
            info.JobMemoryLimit = usize::try_from(cap_bytes).unwrap_or(usize::MAX);
        }
        // SAFETY: job is valid and info matches this information class.
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } != 0;
        // SAFETY: attach callers keep the Child owning raw alive during this call.
        let assigned = configured && unsafe { AssignProcessToJobObject(job, raw as _) } != 0;
        if !assigned {
            let error = failure("apply the Windows job object");
            // SAFETY: this unpublished handle is owned by this function.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
            return Err(error);
        }
        Ok(Self(Some(job)))
    }
}

#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        self.terminate();
    }
}
