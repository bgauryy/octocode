Load when several actors share one machine and any of them builds, runs test suites, starts servers or launches other long jobs.

# Machine budget

Actors on one machine share its cores, memory, disk, build locks and process table, with every other session the person runs. A harness's concurrency limit counts agents, not their jobs: Claude Code stops new subagents at 20 running by default and Codex caps open agent threads per session; neither knows what each agent's compiler or test run costs. Budget the heavy jobs, and let light work run in parallel.

## Read the machine before each launch

Before spawning lanes and again before each heavy job, read (every command only reads):

```bash
# macOS
sysctl -n hw.ncpu; uptime                                         # load against cores
sysctl -n kern.memorystatus_vm_pressure_level vm.swapusage        # 1 normal, 2 warn, 4 critical; swap
top -l 2 -o mem -n 15 -stats pid,ppid,command,mem,cmprs,cpu       # largest footprints; read the second sample
# Linux
nproc; uptime; cat /proc/pressure/memory; free -m
ps -eo pid,ppid,etimes,rss,pcpu,args --sort=-rss | head -20
# both: heavy jobs running now, matched on the executable (adapt the names to the toolchain), then disk
ps -axo pid=,ppid=,pgid=,etime=,%cpu=,comm= | awk '$6 ~ /(^|\/)(cargo|rustc|make|go|javac)$/'
# a runner hosted by a shared interpreter (vitest, pytest) shows as node or python: confirm it by its command line
df -h <workdir>
```

Name each heavy job's owner from its environment marker (below) or its ancestry.

Attributing every process tree to its session and judging leaks is a debugging task: `octocode-debugging` covers it when installed.

## Derive the budget

Derive the number of heavy lanes from these readings each time; do not fix one.

| Work | Starts when | Otherwise |
|---|---|---|
| Heavy: compiles, linters and test suites over a workspace, model servers, browsers, benchmark runs | No other job holds the same build cache or lock; memory pressure is normal and available memory covers the job's measured peak (the last run's footprint per compiler process × its parallel jobs); load is below the core count | Queue it with the build owner. When memory, not the lock, is short, lower the job's own parallelism (`CARGO_BUILD_JOBS`, `make -j`, a test runner's worker count) instead of adding lanes. |
| Light: reading, searching, editing, one focused test against outputs already built | Pressure is not critical | They run in parallel and need no build lock. |

A second build on the same cache gains nothing: Cargo, for one, locks the whole build directory, so the second process waits, and a different package or feature selection compiles another copy of the outputs instead of reusing them. Other sessions' load counts against the budget; report it, never stop it. `make -l` applies the same admission rule to its own jobs: it starts no new job while the load average is at or above the limit.

## Mark, wait and reclaim by owner

- **Mark.** Start every long-running or background process with an owner marker in its environment, such as `AGENT_LANE=<lane> <command>`. Children inherit it and keep it after their parent exits, so a daemon reparented to PID 1 still names its lane. A harness may already export a session marker (Claude Code sets `CLAUDE_CODE_SESSION_ID` and `CLAUDE_PID`); the lane marker separates subagents within one session.
- **Wait** on a PID (`while kill -0 "$pid" 2>/dev/null; do sleep 30; done`) or on the harness's completion notice, never on a command pattern: `pgrep -f "<command>"` matches the waiting shell itself, whose command line contains the pattern, and the loop never ends.
- **Reclaim** at lane end: list the processes carrying the lane's marker (`ps axeww -o pid=,command= | grep -E 'AGENT_LANE=<lane>( |$)'` on macOS; `grep -lsz '^AGENT_LANE=<lane>$' /proc/[0-9]*/environ` on Linux) and the children of the lane's shells. Stop those the lane started, by PID. Name the rest to the orchestrator.

## Report it

The standup adds one row per lane: processes, footprint, current CPU, oldest age, and leak candidates with PID, owner and evidence. Before a heavy job, also compare the build cache's size with free disk. Pruning removes other sessions' warm state and can delete an artifact in use: propose it to the person who owns the machine with the sizes, and never run a full clean while any job is active.

Sources: [Claude Code concurrent subagent limit](https://code.claude.com/docs/en/sub-agents) · [Codex subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents) · [Cargo `build.jobs`](https://doc.rust-lang.org/cargo/reference/config.html#buildjobs) · [Cargo build-dir locking goal](https://rust-lang.github.io/rust-project-goals/2025h2/cargo-build-dir-layout.html) · [GNU make parallel execution](https://www.gnu.org/software/make/manual/html_node/Parallel.html) · [Linux PSI](https://docs.kernel.org/accounting/psi.html)
