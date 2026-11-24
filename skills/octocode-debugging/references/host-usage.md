# Who is using the machine

Use when a host is slow, swapping or short of disk and the cost must be attributed to process trees and their owners before anyone acts. Every command here only reads. Stop only processes you started, by PID; report the rest with their evidence.

## Snapshot the host and its process trees

Name the programs that start sessions on this host (agent CLIs, editors, terminals) as roots, then run the [host snapshot](../scripts/host-usage.mjs) from any directory:

```bash
node "$skill/scripts/host-usage.mjs" --root-name claude --root-name codex
```

It samples twice, `--interval` seconds apart (default 1), plus one `top` pass on macOS that can take several seconds on a loaded host. It reads `ps`, `top`, `sysctl`, `vm_stat` and `launchctl list` on macOS, `/proc` on Linux, and prints one JSON object. Exit 1 means the process table could not be read; exit 2, an invalid option (`--help` lists them). Read the result:

| Field | How to read it |
|---|---|
| `loadExceedsCpus` in `host` | The 1-minute load average is above the CPU count: runnable work is queuing. Linux load also counts tasks in uninterruptible I/O wait. |
| `memory` in `host` | macOS `pressure` normal, warn or critical (the `DISPATCH_MEMORYPRESSURE_*` levels) and `availablePercent`. Linux PSI per resource: `some` is the share of time at least one task stalled, `full` the share all non-idle tasks stalled, over 10, 60 and 300 s. |
| `paging` in `host` | `activePaging` true means swap-outs rose between the samples; a high but static swap total (`swap`) is history. `compressorBytes`, also in `host` on macOS, is the compressor's RAM. |
| `roots` | Groups, largest first. `memoryBytes` sums macOS physical footprints (`top` MEM) or Linux Pss, so shared pages are not double-counted the way summed RSS (`rssBytes`) is; `compressedBytes` is macOS CMPRS. `cpuPercent` is CPU time between the two samples per elapsed second (100 = one core), not the decayed or lifetime average `ps %cpu` shows. The first `--top` groups carry a `tree`; `otherRoots` sums the rest and names the option that shows them. |
| `leakCandidates` | Processes of your user with the `reasons` each was flagged. A candidate is not proof: see the next section. |
| `coverage` | `memoryAsRss` counts processes whose footprint or Pss was unreadable (usually other users' on Linux); their memory is RSS. `cpuUnknown` counts processes that started or exited between samples. |

Disk is separate: `df -h <path>`; `du -sh <dir>/*/`. `du` over a million files takes minutes and loads the disk: run it in the background, once. `memory_pressure` with no arguments prints the same free percentage; its `-l`, `-p` and `-S` options apply or simulate pressure on the whole host: never pass them while measuring.

## Follow one process

macOS:

```bash
ps -o pid,ppid,etime,rss,%cpu,command -g "$pgid"                          # one process group
lsof -a -d cwd -p "$pid" -Fn                                              # working directory
```

Linux:

```bash
pstree -aps "$pid"
readlink "/proc/$pid/cwd"
systemd-cgtop; systemd-cgls                                               # when jobs run in scopes
```

| Gotcha | What to do |
|---|---|
| `ps e` shows nothing for a process | Environments are readable for your own user only; on macOS, platform binaries such as `/bin/zsh` show none. Fall back to ancestry and working directory. |
| `ls`, `grep` are aliases or functions in an agent shell | Check with `type ls`. A decorated listing can trigger expensive metadata work; use `find <dir> -maxdepth 1 -name '<glob>'` on large directories. |
| A search for running jobs lists idle shells | Matching the full command line (`grep cargo`, `pgrep -f`) also finds shells whose command text names the tool. Match the executable instead, as `--root-name` does. |
| A multi-flag command built in a variable does nothing in zsh | zsh does not split an unquoted `$args`; build it as an array. |

## Group, then judge leaks

1. Choose the session roots: agent CLIs, editors, terminals, services. Pass each as `--root-name` (the executable name) or `--root <pid>`; the snapshot assigns every process to its nearest root. When children outlive their parent, group by an owner marker instead: `--marker <VAR>` for a harness session ID or a lane name, `--marker-pid <VAR>` when the variable holds the session's PID. A marker outranks ancestry because children inherit it and keep it after their parent exits and they are reparented to PID 1 (launchd or init).
2. Report the resource measurements and ownership facts needed to explain contention or justify cleanup. Add a marker only when ancestry is insufficient; the snapshot reads only the named variable, never the full environment.
3. Read each candidate's `reasons`: `reparented` (parent is PID 1 and it is not a service), `ownerExited` (the marker names a session process that has exited), `oldChildOfIdleRoot` (older than `--old-after`, default an hour, under a session that used no CPU between the samples). The snapshot does not judge a wait loop or shell whose job ended: read the candidate's command. A wait written as `pgrep -f "<command>"` matches its own shell, whose command line contains the pattern, so it never finishes: wait on a PID instead.
4. Identify a cleanup candidate by PID and verified owner, explaining the evidence that it outlived its task. A detached daemon a tool started on purpose is also `reparented`. The owner decides; a candidate is not proof of a leak.

Sources: [Linux PSI](https://docs.kernel.org/accounting/psi.html) · [proc(5)](https://man7.org/linux/man-pages/man5/proc.5.html) · [ps(1)](https://man7.org/linux/man-pages/man1/ps.1.html) · [systemd-run](https://man7.org/linux/man-pages/man1/systemd-run.1.html) · macOS `man top`, `man ps`, `man memory_pressure`, and `dispatch/source.h` in the SDK.
