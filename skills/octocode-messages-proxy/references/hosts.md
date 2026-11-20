# Agent hosts

`detect` and `ask` find the operator as the nearest agent CLI among the launcher's ancestors. Processes decide; environment variables do not (terminals inherit them, e.g. `CLAUDE_CODE_SSE_PORT` from an editor extension). A Cursor editor ancestor counts only when no agent CLI is found (`via: editor`, an inference). Override a wrong or missing result with `--operator-vendor`.

| Host | Recognized process | Label | Own peer messaging | Proxy host |
|---|---|---|---|---|
| Claude Code | `claude`, or node `@anthropic-ai/claude-code/`; not `--chrome-native-host` | `claude` | `ListAgents`/`SendMessage` | Yes: `haiku` (`claude auth status` → `loggedIn: true`) |
| Codex CLI or app | `codex` (also Codex.app `app-server`), or node `@openai/codex/`; not `mcp-server`/`exec-server` | `codex` | `codex queue --thread UUID` | Yes: first listed Luna, Mini or Nano in `codex debug models` (`codex login status`) |
| Octocode coding agent | title `octocode-<role>-<id>`; the `…/octocode` research binary is not an agent | `octocode` | `agent` type `message` | No |
| Cursor | `cursor-agent`, or its bundled node under `/cursor-agent/` | `cursor` | None | No |
| Grok Build | `grok`, or a binary under `~/.grok/` | `grok` | None | No |
| Pi | `pi`, or node `pi-coding-agent/` | `pi` | Its extension | No |

A running process proves neither a binding nor reachability: the store's `peers` list is the reachable roster. Sign-in does not prove quota: a `Vendor error` such as `usageLimitExceeded` after `ready` ends that proxy.
