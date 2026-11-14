# ghSearchHistory: independently checkable questions

Use the current pinned CLI and a fresh MCP server. Load their schemas before choosing request fields. Questions contain no answer key; campaign oracles and outcomes are separate.

| ID | Case | Question |
|---|---|---|
| GHH-01 | Normal | List the three latest commits affecting cli/cli internal/config/config.go reachable from b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0. Obtain two tool pages with a two-row window and keep the commit identities and subjects. |
| GHH-02 | Advanced | Find merged cli/cli pull requests mentioning config created before 2025-01-01, sorted by comments. Select the first three while retaining their number, title, comment count and stable identifying URL; read the required pages. Separately find closed issues with the same bounded term/date selection. |
| GHH-03 | Empty | Search cli/cli issue history for octocode_benchmark_no_such_issue_20261114. Report a successful empty state or the provider error, without equating them. |
| GHH-04 | Invalid | Send pullRequest with state open and isMerged true, commit with a forbidden glob path, and an unsupported operation. Preserve the errors and repairs. |

For each case save the literal input, natural CLI stdout and stderr, MCP JSON-RPC request and response, exit status, binary identity and selected source identity. Finish emitted response continuations before tool/section continuations and copy their inputs unchanged. Attempt cursor-only resumption on the other surface and refusal of a changed request. A provider error is not an empty answer.

For paired efficiency measurements, use the same selected evidence with ordinary gh, lean projected gh and an explicitly TTL-cached gh api arm where immutable refs permit it. Keep logical request characters separate from environment/setup, preserve failed attempts, and obtain three paired passes in the exclusive timing window. Cache/provider coldness remains unknown without evidence.
