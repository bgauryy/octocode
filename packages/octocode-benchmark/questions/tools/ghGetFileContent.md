# ghGetFileContent: independently checkable questions

Use the current pinned CLI and a fresh MCP server. Load their schemas before choosing request fields. Questions contain no answer key; campaign oracles and outcomes are separate.

| ID | Case | Question |
|---|---|---|
| GHF-01 | Normal | Read cli/cli README.md lines 1–25 at b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0, preserving exact source text and line identity. |
| GHF-02 | Advanced | Read the complete mergeConfig declaration in vitejs/vite packages/vite/src/node/utils.ts at f898888222297aa56d6fc281c585d4d4e4d51f58 with a small response window. Reconstruct all selected lines exactly once. Also inspect its outline and compact view and state their representation changes. |
| GHF-03 | Whole | Read the complete cli/cli internal/config/config.go at b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0 using a small response window, including all Unicode text. Compare reconstructed content against the immutable raw source. |
| GHF-04 | Empty | Look for octocode_benchmark_no_such_symbol_20261114 in cli/cli README.md at b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0; distinguish no matching source lines from an unreadable file. |
| GHF-05 | Invalid | Request endLine before startLine, omit path, and supply a sibling tool field. Retain rejections and corrected calls. |
| GHF-06 | Edge/source | Resolve jquery/jquery tag 3.7.1, then read the window around ajax in dist/jquery.min.js at the immutable resolved commit. Preserve complete selected minified evidence through every byte/response page. Read a small file at the tag and request a directory path to assess its actionable error. |

For each case save the literal input, natural CLI stdout and stderr, MCP JSON-RPC request and response, exit status, binary identity and selected source identity. Finish emitted response continuations before tool/section continuations and copy their inputs unchanged. Attempt cursor-only resumption on the other surface and refusal of a changed request. A provider error is not an empty answer.

For paired efficiency measurements, use the same selected evidence with ordinary gh, lean projected gh and an explicitly TTL-cached gh api arm where immutable refs permit it. Keep logical request characters separate from environment/setup, preserve failed attempts, and obtain three paired passes in the exclusive timing window. Cache/provider coldness remains unknown without evidence.
