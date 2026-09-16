# Native tool runtime architecture

The target runtime has one Rust execution path:

```text
native octocode CLI ──────────┐
                             ├─ Rust runtime / policy / tools
Node MCP → optional NAPI ─────┘           ├─ native config
                                         ├─ providers / registry adapters
                                         └─ octocode-engine-core primitives
```

The CLI never loads NAPI or runs JavaScript. The CLI module lives in the `octocode`
binary crate only; it is not a library or NAPI module. The Node MCP interface owns protocol
framing and generated registration only. It uses the same Rust runtime; no
provider, config, security or LSP implementation lives in JavaScript.

## Current state

The native CLI and optional addon execute the full 11-tool catalog through the
same Rust runtime. Availability matches Node flags: local tools require
`local.enabled` (default on), `ghCloneRepo` requires `ENABLE_CLONE` and
persistent storage, and GitHub/artifact tools are on by default. LSP uses the
shared `octocode-engine-core` language-server client and lifecycle pool.
Config resolution, generated validation, path/content policy and request
lifecycle are native. Local search passes 28 complete CLI and MCP fixture
comparisons, including every continuation. Plain search passes 44 checks and
plain reads pass 35. GitHub file reads pass 35 complete CLI and 35 actual stdio
MCP comparisons, including strict-union input failures and domain continuations.
Four actual CLI batch comparisons pass, with both implementations observing
three simultaneous HTTP requests. ghSearch passes 25 complete CLI envelopes and
25 stdio MCP structuredContent comparisons (code, repositories, tree, and one
cross-operation invalid input). ghGetHistoryItem actual CLI envelopes pass 4/4
on the compact JSON harness. Human commands cover the RFC families: `search`/`read`, `files`/`tree`/`symbols`/`ast`/`graph`/`rewrite`, `def`/`refs`/`callers`/`callees`/`type`/`diagnostics`/`lsp`, `repos`/`clone`/`package`/`history`, plus `context`, `status`, `auth`, `login`, `logout`, `cache`, and `skill`. Interactive IDE install and OAuth TUI remain Node `octocode` management.

NAPI admits requests synchronously before scheduling futures, so cancellation
cannot race the first Rust poll. Admission is bounded and owns cleanup through
completion. Credential acquisition runs inside admitted blocking work and owns
a thread handle that must join; the pooled HTTP client receives an already
pinned credential shared by the whole batch. The batch owns an ordered stream
of at most three in-flight HTTP operations, without detached tasks or extra
per-query thread pools. Provider content uses the common bounded cache, partitioned
by endpoint, credential and session identity. Final response sanitization also
covers metadata and remote error strings.

GitHub search is catalog-available through the same pooled HTTP client as file
reads. Its request builder preserves canonical keyword quoting, file-path
splitting, repository filters and media types. Shared portable minification and
UTF-16 re-anchoring keep snippet match positions attached to transformed text.
ICU4X collation replaces JavaScript locale comparison for repository ranking;
its compiled data is initialized once. The additional dependency size and locale
matrix remain measurement gates. Tree listings share the GitHub content cache
(ETag conditional GET, optional disk persist under `{OCTOCODE_HOME}/tmp/response`
when storage is persistent). Crate tests are Tokio/`cargo test` against
`ToolRuntime` and the `octocode` binary. They do not spawn Python or the frozen
Node CLI.
Structural rewrite's baseline uses the attested native ast-grep executable.
The embedded ast-grep experiment is behind core's optional
`embedded-ast-grep-rewrite` feature and is excluded from `portable-default`.

Canonical instructions are generated for enabled-tool combinations and selected
in Rust. Embedded contracts are parsed once into immutable data. Prepare fills
envelope meta fields only; it does not alias tool fields. Human CLI commands
emit canonical query shapes. Node forwards registration, instructions, request
arguments and cancellation only.

The crate builds binaries without addon features and builds the addon as a
library with `napi-addon`. Advanced ECMAScript patterns use a separately bounded
Rust helper; its integration and platform resource checks are still in progress.
The helper is counted in process-tree latency/CPU/memory, never hidden as free
work. Darwin RSS enforcement is sampled rather than a hard allocation ceiling.

The frozen first-read release experiment measured 10.3–13.4× lower median
process latency across four small read cases. This is a scoped observation, not
a migration-wide speed claim; raw measurements and limitations are recorded in
`.octocode/implementation/rust-migration/FIRST-READ-COMPARISON.md` at the repo root.
Current-candidate resource and platform comparisons remain release gates.

## Ownership

| Module | Owns | May not depend on |
|---|---|---|
| config | Home/env acquisition, parsing, validation, resolution and diagnostics | Engine, providers, tools, NAPI |
| contracts | Generated canonical schemas/rules and native input validation | Tool execution or Node validators |
| runtime/policy/cache | Dispatch, request context, security, cancellation and resource limits | CLI formatting or MCP SDK types |
| tools | Canonical operations using shared services | Independent config/auth/security implementations |
| providers/registries | Remote DTOs, HTTP/retry/cache/credential policy | CLI or MCP framing |
| lsp | Runtime composition in `src/lsp`; tools call the core-owned pool as a shared service | A second lifecycle policy, TypeScript engine wrappers, CLI, NAPI types |
| adapter_napi | Host conversion, runtime handle and lifecycle | A separate execution implementation |
| CLI | Arguments, human output and shell exits (`octocode` binary crate; not a lib module) | Node runtime, NAPI, or the addon cdylib |

Reusable engine Rust algorithms live in `octocode-engine-core`, which this crate
consumes directly without a Cargo dependency on `octocode-engine`. Core's portable
APIs accept resolved options and do not import this higher-level runtime. The
published engine package separately adapts the same core through optional NAPI
bindings. The public tool core remains the contract authoring owner;
build-time generation produces Rust artifacts. Candidate generation must fail on
unsupported executable rules rather than omit them.

Config is acquired fresh into explicit inputs. Secrets remain private; no
process-wide environment mutation occurs from concurrent native tasks. Runtime
handles own persistent pools and registries across MCP calls and close them
explicitly. Bounded queues, cancellation, partial results and executable
continuations are part of each operation's contract.

The frozen Node reference uses separate source, dependencies, homes, caches,
journals and artifact outputs. Unimplemented native operations fail explicitly.
Quality, latency, CPU and whole-process memory are measured on matched inputs;
the candidate is not promoted until its applicable gates pass.
