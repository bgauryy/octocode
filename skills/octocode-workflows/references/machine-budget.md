Load when actors share a machine and run builds, suites, servers or other resource-intensive jobs.

# Machine budget

Before adding heavy work, read the host with [machine-snapshot.mjs](../scripts/machine-snapshot.mjs): `node <skill-folder>/scripts/machine-snapshot.mjs --disk <build output dir>`. Read the project's own build-lock or slot status beside it. Agent-thread limits do not describe compiler, browser or model-server capacity. The snapshot includes other sessions' load; it stops nothing.

| Field | Read it as |
|---|---|
| `load.perCpu` | Above 1: runnable work already exceeds the CPUs |
| `memory.pressure.level` (macOS), `memory.pressure.psi.some` (Linux) | `warn` or `critical`; on Linux, `avg10` (percent of the last 10 s tasks stalled on memory) above `avg60` means stalls are rising. Either: lower the job's parallelism before adding work |
| `memory.availableBytes`, `disk[].freeBytes` | Compare with the job's observed peak memory and output size |
| `notes` | Readings that were unavailable: unknown, not spare capacity |

## Admit work from observed capacity

| Work | Admission rule |
|---|---|
| Heavy build, workspace suite, browser or model server | Give shared outputs and build locks one owner. Queue conflicting jobs; size parallelism from available resources and the workload's observed peak. Lower the job's own parallelism when memory is the constraint. |
| Reading, editing or a focused check against built outputs | Run concurrently while it does not worsen resource contention. |

Take the snapshot when admitting heavy work, not before every read or edit. `octocode-debugging`, when installed, attributes memory and CPU to individual process trees.

## Own process lifetime

- Retain the PIDs of processes you start. Add an owner/session marker when needed to distinguish shared-machine jobs; reuse a marker the host already provides.
- Wait on the task's completion notification or its actual PID, never a command pattern that can match the waiting shell itself.
- At completion, run `node <skill-folder>/scripts/machine-snapshot.mjs --children <pid>` for each process you started and keep every `children.running[].pid` (it exits 1 while they run). Stop only your own processes by their recorded PIDs, then rerun it with all of those PIDs, comma-separated: exit 0 with `allExited: true` confirms the tree is gone. Exit 1 lists survivors in `running`; `relation` shows whether each is the named PID, a descendant or a member of its process group. `zombies` have exited and wait for their parent to reap them. Report unowned leak candidates rather than killing them.
- Preserve artifacts in use. Propose cache pruning to its owner with actual disk evidence; never clean a shared build cache during an active job.

Report contention, the affected work and its next action. Add process/resource details only where they explain the blocker or support a measurement.

Sources: [Cargo build jobs](https://doc.rust-lang.org/cargo/reference/config.html#buildjobs) · [Cargo build-directory locking](https://goals.rust-lang.org/2025h2/cargo-build-dir-layout.html) · [GNU make parallel execution](https://www.gnu.org/software/make/manual/html_node/Parallel.html) · [Linux memory pressure](https://docs.kernel.org/accounting/psi.html)
