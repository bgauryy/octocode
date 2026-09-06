# Shared Awareness Flow

Load when choosing between shared coordination outcomes. This reference step ends here; return to the main skill flow.

Use `npx @octocodeai/octocode-awareness`. Host tools and hooks may perform the same operations through the package API. One root vocabulary covers the ledger: `plan`, `task`, `work`, `lock`, `verify`, `memory`, `agent`, and `signal`; use `verify`, not `check`, and `signal`, not `message`. Run `schema commands --compact` and `schema command <noun> [action]` for exact flags.

| Trigger | Action | Expected output / close |
|---|---|---|
| Orient | `attend`, `status`, `query <view>` | Bounded state and targeted rows; reads do not mutate. |
| Plan work | `plan list/show/status`, `task ready/claim`, `work list/show` | Inspect one decision-changing row before claiming or declaring. |
| Declare and protect | `work start/touch/end`, `lock acquire/wait/release/prune` | Presence is advisory; locks are exceptional and expiry is not success. |
| Coordinate | `agent list/register`, `signal list/publish/reply/ack/resolve`, `session capture` | Durable peer evidence or a scoped continuation capture. |
| Finish | `task submit/release`, `work end`, `verify mark/audit` | Run the declared check; only an observed receipt proves success. |
| Learn | `memory recall/record/forget/archive/restore`, `refinement get/set/delete`, `reflect record` | Memory is a verified lead; follow-up has an owner and terminal receipt. |
| Inspect and maintain | `docs list/show/staleness`, `schema ...`, `config ...`, `maintenance ...`, `hooks ...`, `hook run` | Load one doc or schema; preview cleanup and hook mutations. |

Return to `SKILL.md` after choosing the shared outcome. Reads are observational; explicit mutation commands reclaim or prune stored rows.
