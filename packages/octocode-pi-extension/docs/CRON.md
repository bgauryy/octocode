# Octocode Session Jobs

`@octocodeai/pi-extension` runs lightweight session-scoped jobs while Pi is open.
They are deliberately **not** OS cron: timers start on `session_start` and are
cleared on `session_shutdown`, `/new`, reload, resume, fork, or quit.

## Safety model

- Default jobs are report-first and non-mutating.
- The built-in status job runs Awareness read-only `status`.
- No job calls a model.
- Jobs never run when `OCTOCODE_CRON=0`.
- Mutating cleanup such as `memory forget` or `wiki sync` remains manual.

## Commands

```text
/octocode-cron list
/octocode-cron check [default|all|job]
/octocode-cron cancel [default|all|job]
/octocode-cron help
```

`list` shows every session job. Bare `check` runs the default job
(`awareness-status`); `check all` runs every registered job; `check <job>` runs
one named job. `cancel` follows the same target rules and disables timers for the
current session only.

## Default job

| Job | Interval | Action |
|---|---:|---|
| `awareness-status` | 30 min | `octocode-awareness` bin from installed `@octocodeai/octocode-awareness`: `status --workspace <cwd>` |

## Configuration

| Variable | Default | Effect |
|---|---:|---|
| `OCTOCODE_CRON` | `1` | Set `0` to disable all session jobs. |
| `OCTOCODE_CRON_STATUS` | `1` | Set `0` to leave the status job unscheduled. |
| `OCTOCODE_CRON_STATUS_INTERVAL_MS` | `1800000` | Override status interval in milliseconds. |

The scheduler invokes the installed scoped package CLI directly with the current Node runtime. Manual and scheduled
runs report command failures visibly instead of failing the session.
