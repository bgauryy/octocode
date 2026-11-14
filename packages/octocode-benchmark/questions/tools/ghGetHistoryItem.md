# ghGetHistoryItem: independently checkable questions

Use the current pinned CLI and a fresh MCP server. Load their schemas before choosing request fields. Questions contain no answer key; campaign oracles and outcomes are separate.

| ID | Case | Question |
|---|---|---|
| GHI-01 | Normal | For the newest path commit discovered in GHH-01, read the complete patch only for internal/config/config.go and retain parent/commit identity. Use the discovered immutable commit, never a guessed one. |
| GHI-02 | Advanced | For a merged pull request discovered in GHH-02, read its body, every selected file and review using small response windows. Walk response pages before section pages. Compare complete selected collections with the provider and disclose unavailable patches. |
| GHI-03 | Issue | Read the body and complete comments of one issue discovered in GHH-02, keeping source identities and sequence. Compare all selected comments with a paginated provider read. |
| GHI-04 | Compare | Compare cli/cli b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0 with its first parent for internal/config/config.go only, keeping complete selected patches or an honest unchanged-file result. |
| GHI-05 | Invalid | Send a pullRequest without number, a compare without headRef, and sections body together with include. Preserve the actionable input responses. |
| GHI-06 | Summary/sections | Read a discovered merged pull request summary, then separately its files, reviews and one path patch. Confirm section reads retain number/state/commit identity and that independent section continuations cover complete requested evidence. |

For each case save the literal input, natural CLI stdout and stderr, MCP JSON-RPC request and response, exit status, binary identity and selected source identity. Finish emitted response continuations before tool/section continuations and copy their inputs unchanged. Attempt cursor-only resumption on the other surface and refusal of a changed request. A provider error is not an empty answer.

For paired efficiency measurements, use the same selected evidence with ordinary gh, lean projected gh and an explicitly TTL-cached gh api arm where immutable refs permit it. Keep logical request characters separate from environment/setup, preserve failed attempts, and obtain three paired passes in the exclusive timing window. Cache/provider coldness remains unknown without evidence.
