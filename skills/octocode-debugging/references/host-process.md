Load when one process or process group from the [host snapshot](host-usage.md) needs its ancestry, working directory or owner, or a process listing behaves unexpectedly.

# Follow one process on the host

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

Sources: [proc(5)](https://man7.org/linux/man-pages/man5/proc.5.html) · [ps(1)](https://man7.org/linux/man-pages/man1/ps.1.html) · [systemd-run](https://man7.org/linux/man-pages/man1/systemd-run.1.html) · macOS `man ps`.
