# Go diagnostics

## Already running

If the service already serves private `net/http/pprof` handlers, verify the instance and use its configured base URL (for example `http://127.0.0.1:6060/debug/pprof`). A load-balanced URL can switch instances between requests. Set `base`, an existing private output directory `out`, and a positive window `seconds`. Drive the workload **during** each recording. Choose one capture at a time.

| Question | Capture | Inspect |
|---|---|---|
| Which code runs on CPU? | `curl -fsS "$base/profile?seconds=$seconds" -o "$out/cpu.pprof"` | `go tool pprof -top "$out/cpu.pprof"` for flat cost, then `go tool pprof -top -cum "$out/cpu.pprof"` for callers. |
| Where are goroutines stalled? | `curl -fsS "$base/goroutine?debug=2" -o "$out/goroutines.txt"` during the stall. Repeat to a new file. | Follow blocked goroutines to the lock, channel or dependency that can wake them. A parked background goroutine can be healthy. |
| When did work run or wait? | `curl -fsS "$base/trace?seconds=$seconds" -o "$out/trace.out"` | `go tool trace "$out/trace.out"` for goroutine transitions, scheduler, syscalls and GC. In batch, `go tool trace -pprof=sched "$out/trace.out" > "$out/sched.pprof"`, then inspect with pprof. Scheduler latency measures runnable time awaiting execution. |
| What is retained or allocated? | `curl -fsS "$base/heap" -o "$out/heap.pprof"` at comparable GC/workload points | `go tool pprof -top -inuse_space "$out/heap.pprof"` shows sampled live bytes at the recorded GC boundary. `-alloc_space` shows cumulative allocated bytes including freed objects. |

For a request timeline, select its user task, regions and goroutine in the trace viewer. Read the state transitions from running to waiting, runnable and running again. Time in the runnable state measures scheduling delay. Timers and blocked dependencies occupy different intervals. For batch analysis, check `go tool trace -help` for the installed version's event dump mode (for example `-d=parsed`), then join task markers, goroutine transitions and wakeups within that trace. The debug dump format varies by version. [Go trace annotations](https://pkg.go.dev/runtime/trace)

If enabled, the block profile attributes synchronization delay to waiting stacks. The mutex profile attributes contention to holder stacks at unlock. Record the profile rates and interval. With no diagnostics endpoint, use an authorized OS sampler or reproduce under instrumentation. Go's default Unix SIGQUIT handler dumps stacks **and exits**. Use the goroutine endpoint for a live dump. [HTTP profile handlers](https://pkg.go.dev/net/http/pprof) · [Go profile semantics](https://pkg.go.dev/runtime/pprof) · [Execution traces](https://pkg.go.dev/cmd/trace) · [Go signal behavior](https://pkg.go.dev/os/signal)

## Launch

From one target package directory, set `out` to an absolute directory and replace `BenchmarkWork` with the project's representative benchmark. Keep input and concurrency comparable to the service.

```bash
go test -run '^$' -bench '^BenchmarkWork$' -cpuprofile "$out/cpu.pprof" -o "$out/pkg.test" .
go tool pprof -top "$out/pkg.test" "$out/cpu.pprof"
```

For scheduling, run the same workload separately with `-trace "$out/trace.out"` in place of `-cpuprofile`. Preserve the binary for symbolization. For a wrong value use Delve on a disposable debug build (`dlv debug` or `dlv test`). A live `dlv attach PID` can pause the service. Go goroutine IDs and OS thread IDs identify different things. Use request/task annotations to follow a request through goroutine handoffs. [Go diagnostics](https://go.dev/doc/diagnostics) · [Test flags](https://pkg.go.dev/cmd/go#hdr-Testing_flags)
