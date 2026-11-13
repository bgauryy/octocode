# Who is using the machine

Use when a host is slow, swapping or short of disk and the cost must be attributed to process trees and their owners before anyone acts. Every command here only reads. Stop only processes you started, by PID; report the rest with their evidence.

## Host signals

| Question | macOS | Linux | How to read it |
|---|---|---|---|
| CPU queue | `sysctl -n hw.ncpu; uptime` | `nproc; uptime` | A load average above the core count means runnable work is queuing. Linux load also counts tasks in uninterruptible I/O wait. |
| Memory pressure | `sysctl -n kern.memorystatus_vm_pressure_level kern.memorystatus_level` | `cat /proc/pressure/memory /proc/pressure/cpu` | macOS level 1, 2, 4 are normal, warn, critical (the values of `DISPATCH_MEMORYPRESSURE_*`); `memorystatus_level` is the percent available. PSI `some` is the share of time at least one task stalled, `full` the share all non-idle tasks stalled, over 10, 60 and 300 s. |
| Paging | `sysctl -n vm.swapusage`, then `vm_stat` twice | `free -m; vmstat 1 5` (`si`, `so`) | Swapouts rising between two reads is active paging; a high but static swap total is history. `vm_stat` counts pages: multiply by the page size in its header. "Pages occupied by compressor" is the compressor's RAM. |
| Disk | `df -h <path>`; `du -sh <dir>/*/` | same | `du` over a million files takes minutes and loads the disk: run it in the background, once. |

`memory_pressure` with no arguments prints the same free percentage. Its `-l`, `-p` and `-S` options apply or simulate pressure on the whole host: never pass them while measuring.

## Attribute to process trees

macOS:

```bash
top -l 2 -o mem -n 30 -stats pid,ppid,pgrp,command,mem,cmprs,cpu,time   # read the second sample
ps -axo pid,ppid,pgid,etime,rss,%cpu,command                             # tree, age, command
ps -o pid,ppid,etime,rss,%cpu,command -g "$pgid"                          # one process group
lsof -a -d cwd -p "$pid" -Fn                                              # working directory
ps -axo pid=,ppid=,etime=,%cpu=,comm= | awk '$5 ~ /(^|\/)cargo$/'         # running jobs by executable name
```

Linux:

```bash
top -b -n 2 -d 1 -o %MEM | awk '/^top -/{n++} n==2' | head -40          # second iteration is current
ps -eo pid,ppid,pgid,sid,etimes,rss,pcpu,args --sort=-rss
pstree -aps "$pid"
readlink "/proc/$pid/cwd"
grep -E '^(Rss|Pss)' "/proc/$pid/smaps_rollup"
systemd-cgtop; systemd-cgls                                               # when jobs run in scopes
```

| Gotcha | What to do |
|---|---|
| macOS RSS omits compressed and swapped pages | Rank by `top` `MEM` (physical footprint) and `CMPRS`. |
| `top`'s first sample has no CPU delta (macOS shows 0.0) | Use `-l 2` / `-n 2` and read the last sample. `ps %cpu` is a decayed (macOS) or lifetime (Linux) average, not current use. |
| Summed RSS double-counts shared pages | On Linux sum `Pss`; on macOS compare footprints, which count each process's own dirty and compressed pages. |
| `ps e` shows nothing for a process | Environments are readable for your own user only; on macOS, platform binaries such as `/bin/zsh` show none. Fall back to ancestry and working directory. |
| `ls`, `grep` are aliases or functions in an agent shell | Check with `type ls`. A decorated listing can trigger expensive metadata work; use `find <dir> -maxdepth 1 -name '<glob>'` on large directories. |
| A search for running jobs lists idle shells | Matching the full command line (`grep cargo`, `pgrep -f`) also finds shells whose command text names the tool. Match the executable instead: the `comm` column, as in the last macOS line above. |
| A multi-flag command built in a variable does nothing in zsh | zsh does not split an unquoted `$args`; build it as an array. |

## Group, then judge leaks

1. Find the session roots: agent CLIs, editors, terminals, services. Assign every process to the nearest root. An owner marker in the environment (a harness session ID or a lane name) outranks ancestry, because children inherit it and keep it after their parent exits and they are reparented to PID 1 (launchd or init).
2. Report the resource measurements and ownership facts needed to explain contention or justify cleanup. Inspect an owner marker only when ancestry is insufficient; avoid dumping the full environment.
3. A **leak candidate** is any of: reparented to PID 1 and not a known service; carrying an owner marker that names a session process that has exited (`ps -p "$owner_pid"` prints nothing); a wait loop or shell whose job ended; an hours-old child of an idle session. A wait written as `pgrep -f "<command>"` matches its own shell, whose command line contains the pattern, so it never finishes: wait on a PID instead.
4. Identify a cleanup candidate by PID and verified owner, explaining the evidence that it outlived its task. The owner decides; a candidate is not proof of a leak.

Sources: [Linux PSI](https://docs.kernel.org/accounting/psi.html) · [proc(5)](https://man7.org/linux/man-pages/man5/proc.5.html) · [ps(1)](https://man7.org/linux/man-pages/man1/ps.1.html) · [systemd-run](https://man7.org/linux/man-pages/man1/systemd-run.1.html) · macOS `man top`, `man ps`, `man memory_pressure`, and `dispatch/source.h` in the SDK.
