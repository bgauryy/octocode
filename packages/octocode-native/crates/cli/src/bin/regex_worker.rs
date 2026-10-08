// Test code may assert with unwrap/expect/panic.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]

use std::io::{Read, Write};

use octocode_native::regex::{
    MatchRange, REGEX_WORKER_PROTOCOL_VERSION, WorkerOperation, WorkerRequest, WorkerResponse,
};

const MAX_STDIN_BYTES: usize = 64 * 1024;

fn main() {
    let response = constrain_self()
        .and_then(|()| run())
        .unwrap_or_else(|error| WorkerResponse {
            version: REGEX_WORKER_PROTOCOL_VERSION,
            ranges: None,
            error: Some(error),
        });
    let encoded = serde_json::to_vec(&response).unwrap_or_else(|_| {
        br#"{"version":1,"ranges":null,"error":"serialization failure"}"#.to_vec()
    });
    let _ = std::io::stdout().write_all(&encoded);
}

/// Resource limits are positional arguments set by the parent
/// (`IsolatedRegexEngine`): `<max-memory-bytes> <max-cpu-seconds>`.
fn configured_limit(position: usize, name: &str) -> Result<u64, String> {
    std::env::args()
        .nth(position)
        .ok_or_else(|| format!("Missing required worker resource limit: {name}"))?
        .parse::<u64>()
        .map_err(|_| format!("Invalid worker resource limit: {name}"))
        .and_then(|value| {
            (value > 0)
                .then_some(value)
                .ok_or_else(|| format!("Worker resource limit must be positive: {name}"))
        })
}

#[cfg(unix)]
fn constrain_self() -> Result<(), String> {
    let memory = configured_limit(1, "max-memory-bytes")? as libc::rlim_t;
    let cpu = configured_limit(2, "max-cpu-seconds")? as libc::rlim_t;
    #[cfg(target_os = "macos")]
    let limits_to_apply = vec![(libc::RLIMIT_CPU, cpu, "cpu")];
    #[cfg(not(target_os = "macos"))]
    let limits_to_apply = vec![
        (libc::RLIMIT_AS, memory, "memory"),
        (libc::RLIMIT_CPU, cpu, "cpu"),
    ];
    #[cfg(target_os = "macos")]
    let _ = memory;
    for (resource, limit, label) in limits_to_apply {
        let mut limits = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: getrlimit initializes the provided current-process structure.
        if unsafe { libc::getrlimit(resource, &raw mut limits) } != 0 {
            return Err(format!(
                "Unable to inspect regex worker {label} limit: {}",
                std::io::Error::last_os_error()
            ));
        }
        #[cfg(target_os = "macos")]
        if limits.rlim_max == i64::MAX as libc::rlim_t {
            // Darwin reports infinity through the signed rlim_t sentinel while
            // setrlimit expects RLIM_INFINITY's unsigned representation.
            limits.rlim_max = libc::RLIM_INFINITY;
        }
        limits.rlim_cur = limit.min(limits.rlim_max);
        // SAFETY: setrlimit reads this initialized structure and applies it to
        // the current helper process before any untrusted regex is parsed.
        if unsafe { libc::setrlimit(resource, &limits) } != 0 {
            return Err(format!(
                "Unable to apply regex worker {label} limit: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn constrain_self() -> Result<(), String> {
    use std::ffi::c_void;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOB_OBJECT_LIMIT_PROCESS_TIME,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let memory = configured_limit(1, "max-memory-bytes")? as usize;
    let cpu = configured_limit(2, "max-cpu-seconds")?;
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY
        | JOB_OBJECT_LIMIT_PROCESS_TIME
        | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    limits.BasicLimitInformation.PerProcessUserTimeLimit = (cpu.saturating_mul(10_000_000)) as i64;
    limits.ProcessMemoryLimit = memory;
    // SAFETY: all handles and pointers are process-local, initialized, and used
    // before input is read. Any failure prevents regex execution.
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Err(format!(
                "Unable to create regex worker Job Object: {}",
                std::io::Error::last_os_error()
            ));
        }
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast::<c_void>(),
            std::mem::size_of_val(&limits) as u32,
        ) == 0
            || AssignProcessToJobObject(job, GetCurrentProcess()) == 0
        {
            return Err(format!(
                "Unable to attach regex worker Job Object: {}",
                std::io::Error::last_os_error()
            ));
        }
        // The OS closes the job handle at process exit. Keeping it open retains
        // KILL_ON_JOB_CLOSE for the helper lifetime.
        let _job_lifetime_handle = job;
    }
    Ok(())
}

fn run() -> Result<WorkerResponse, String> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take((MAX_STDIN_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_STDIN_BYTES {
        return Err("Regex request exceeds worker byte limit".into());
    }
    let request: WorkerRequest =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if request.version != REGEX_WORKER_PROTOCOL_VERSION {
        return Err("Unsupported regex worker protocol version".into());
    }
    let regex = regress::Regex::with_flags(&request.source, request.flags.as_str())
        .map_err(|error| error.to_string())?;
    match request.operation {
        WorkerOperation::Find { max_matches } => {
            let ranges = regex
                .find_iter(&request.input)
                .take(max_matches)
                .map(|matched| MatchRange {
                    start: matched.start(),
                    end: matched.end(),
                })
                .collect();
            Ok(WorkerResponse {
                version: REGEX_WORKER_PROTOCOL_VERSION,
                ranges: Some(ranges),
                error: None,
            })
        }
    }
}
