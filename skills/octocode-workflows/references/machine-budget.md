Load when actors share a machine and run builds, suites, servers or other resource-intensive jobs.

# Machine budget

Read current load, memory pressure, available memory, free disk and active build locks before adding heavy work. Agent-thread limits do not describe compiler, browser or model-server capacity. Include other sessions' load without stopping their processes.

## Admit work from observed capacity

| Work | Admission rule |
|---|---|
| Heavy build, workspace suite, browser or model server | Give shared outputs and build locks one owner. Queue conflicting jobs; size parallelism from available resources and the workload's observed peak. Lower the job's own parallelism when memory is the constraint. |
| Reading, editing or a focused check against built outputs | Run concurrently while it does not worsen resource contention. |

Use native monitoring tools for the host (`top`, `ps`, memory-pressure reporting and disk usage); inspect the jobs relevant to the decision rather than taking a fixed sample or running a monitoring ritual before every read. `octocode-debugging`, when installed, covers detailed process attribution.

## Own process lifetime

- Retain the PIDs of processes you start. Add an owner/session marker when needed to distinguish shared-machine jobs; reuse a marker the host already provides.
- Wait on the task's completion notification or its actual PID, never a command pattern that can match the waiting shell itself.
- At completion, stop only your own processes by their recorded PIDs and verify that their children have exited. Report unowned leak candidates rather than killing them.
- Preserve artifacts in use. Propose cache pruning to its owner with actual disk evidence; never clean a shared build cache during an active job.

Report contention, the affected work and its next action. Add process/resource details only where they explain the blocker or support a measurement.

Sources: [Cargo build jobs](https://doc.rust-lang.org/cargo/reference/config.html#buildjobs) · [Cargo build-directory locking](https://rust-lang.github.io/rust-project-goals/2025h2/cargo-build-dir-layout.html) · [GNU make parallel execution](https://www.gnu.org/software/make/manual/html_node/Parallel.html) · [Linux memory pressure](https://docs.kernel.org/accounting/psi.html)
