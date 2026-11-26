Load when capturing native stacks from a PID with the capture script, choosing the macOS, Linux or Windows profiler, or profiling a JVM, .NET or other managed runtime.

# Platform and managed-runtime captures

## Platform captures

The [capture script](../scripts/native-capture.sh) `bash "$skill/scripts/native-capture.sh" "$pid" "$seconds" "$out/stacks"` attaches to one verified, authorized PID (not a process tree) and neither stops nor signals it. It uses macOS `sample` or Linux `perf`, refuses an existing output directory, and fails visibly on permission or tool errors. It records PID/PPID, command name, duration and diagnostics. In `process.txt`, RSS is KiB and `%cpu` is OS-reported, not a capture-window measurement. Verify process identity before and after the capture; discard it if the PID was reused or the process exited early. macOS `sample` includes sleeping threads: pair it with process CPU deltas or Instruments Time Profiler for on-CPU attribution.

| Platform | Live target | Launch or inspect |
|---|---|---|
| macOS | The capture script, read with the sleeping-thread caveat above. | Use Instruments Time Profiler on a launched app for CPU attribution. Inspect thread and time range. Use System Trace for scheduling if available. |
| Linux | `pidstat -t -u -w -p "$pid" 1 5` shows threads, CPU and context switches. The capture script uses perf on a PID. Check its event and process coverage. A host CPU average can hide a worker starved by its cgroup quota. | `perf record --call-graph dwarf -o "$out/perf.data" -- ./app` then `perf report --stdio -i "$out/perf.data"`. For scheduler latency use a scheduling trace. For cgroup v2 check deltas in the target's `cpu.stat`. |
| Windows | Select the relevant WPR profile and record the affected interval. System-wide ETW collection can include other processes. | Stop and save the ETL, then filter by process, thread and interval in WPA. CPU Usage (Sampled) shows stacks. With scheduling events, CPU Usage (Precise) can distinguish a thread ready for CPU from one blocked on a dependency. |

[Linux perf record](https://man7.org/linux/man-pages/man1/perf-record.1.html) · [pidstat](https://man7.org/linux/man-pages/man1/pidstat.1.html) · [Linux cgroup v2](https://docs.kernel.org/admin-guide/cgroup-v2.html) · [Apple Instruments](https://developer.apple.com/tutorials/instruments) · [Windows Performance Recorder](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/windows-performance-recorder) · [WPA CPU analysis](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/cpu-analysis)

## Other managed runtimes

- **JVM.** For a live JVM with JDK diagnostic access, check `jcmd "$pid" help JFR.start`, then `jcmd "$pid" JFR.start name=investigation settings=profile duration=30s filename="$out/run.jfr"` using an absolute path writable by the target. For a launch, put `-XX:StartFlightRecording=settings=profile,filename=run.jfr,dumponexit=true` before the application arguments. Inspect the recording with JDK Mission Control or `jfr summary run.jfr`. Compare execution samples with GC, parks and monitor events enabled in that recording. [JDK diagnostic commands](https://docs.oracle.com/en/java/javase/25/docs/specs/man/jcmd.html) · [JFR tool](https://docs.oracle.com/en/java/javase/25/docs/specs/man/jfr.html)
- **.NET.** With dotnet-trace installed, use `dotnet-trace collect -p "$pid" --duration 00:00:00:10 -o "$out/run.nettrace"`, or `dotnet-trace collect -o "$out/run.nettrace" -- dotnet App.dll` for a finite launch. Check the tool's profiles/providers before capture. Inspect sampled managed stacks and runtime events in the trace. For a hang, `dotnet-stack report -p "$pid"` gives a point-in-time managed stack for each thread. [dotnet-trace](https://learn.microsoft.com/en-us/dotnet/core/diagnostics/dotnet-trace) · [dotnet-stack](https://learn.microsoft.com/en-us/dotnet/core/diagnostics/dotnet-stack)
- **Other runtimes.** OS counters and stacks still locate the busy or waiting process. Use the runtime's supported sampler or request spans when native symbols stop at a VM, JIT or executor. Label what remains opaque.
