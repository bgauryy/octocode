# Native tool runtime architecture

`octocode-native` is the sole public tool execution owner and the npm distribution owner for the native binary (CLI and MCP server).

```text
octocode <tool> (CLI) ────────┐
                             ├──▶ Rust ToolRuntime
octocode mcp (stdio MCP) ─────┘       ├── generated octocode-core contracts
                                      ├── config and security policy
                                      ├── providers, credentials, and caches
                                      ├── tool orchestration and responses
                                      └── octocode-engine primitives
```

Neither interface loads JavaScript. The CLI commands and `octocode mcp` run in the same binary and call the same Rust runtime. The Node launchers (`octocode`, `octocode-mcp`) own process startup only; Rust owns command parsing, output, interactive selection, skills, the browser configuration server and the MCP server. `octocode mcp` (rmcp) owns protocol framing for both eras (2025-11-25 `initialize`, 2026-07-28 `server/discover`) and lists tools from the embedded presentation; execution, validation and response shaping are the runtime's.

GitHub materialization separates acquisition from persistence. `tools/gh_structure/materialize`
owns listing order, provider reads, limits, and continuations; its `snapshot` module owns
the v3 snapshot manifest, SHA-256 verification, and a kernel file lock held through
publication. The lock coordinates batch rows and native processes; its asynchronous wait
uses the request deadline and cancellation budget. The shared `private_file::BeneathDir`
capability creates descendants and atomically replaces files through held directory
descriptors on Unix, refusing ancestor links and replacing a leaf link itself.
Windows retains the checked directory chain, rejects reparse points, and compares
volume/file identities when checking the held directory. Child opens and creates use
`NtCreateFile` relative to the held parent; renames and removals operate on those handles
through `NtSetInformationFile`. Directory sharing restrictions also prevent renames;
confinement does not depend on those restrictions blocking attribute-only changes.
Other non-Unix targets retain the checked-path fallback.

## Public catalog

The runtime owns all fourteen catalog tools ([tools reference](../../docs/OCTOCODE_TOOLS.md)). Availability is resolved once per runtime from configuration and surface, without a provider startup request (`execution/availability.rs`). Each tool is `Available` or carries one reason: `Disabled` (the contract parks it on every surface), `ToolList` (`tools.enabled` / `tools.disabled`), `CliOnly`, or `MissingConfiguration` with the unset settings in order. The catalog, admission, clasify context errors, and CLI help and schema all project that one value. GitHub and artifact tools are enabled by default; local tools honor local policy. `ghCloneRepo` is CLI-only and needs persistent storage. Public `clasify` needs beta and a nonblank classification credential (`OCTOCODE_CLASSIFICATION_API` or `classification.api`). Optional `clasifyContext` belongs only to `localFetch`, `localSearch`, `ghGetFileContent`, and `ghSearchCode`, and only when that credential is present, whatever the beta state. The [availability guide](../../docs/OCTOCODE_CLASIFY.md#availability) owns the gate combinations. Public tool input accepts a bare query object or `{ "queries": [...] }`; a bare array is rejected.

`clasify` exposes model discovery, whole-resource evaluation and section assessment. An evaluation row holds keyed `noul`/`choice`/`score` questions over exactly one literal `state` or one `source`, with image assets available for supported primitives. `tools/source_capture` captures a source completely through the same authorized readers a direct call runs (contract validation, input security, the reader's path policy and redaction, output sanitization), following every content page pinned to the first page's file snapshot or commit; a selection over the state limit (`clasify.semanticPolicy.maxStateChars`) fails its row instead of being judged in part. A URL source is fetched over HTTPS under the address policy and saved as a capture file that a `localFetch` lead reads back. The row returns the provider's validated answers unchanged, the model that answered, its usage, and for a source one `read` suggestion in `hints` of exactly what was judged; never a body. `mainGoal` and `reasoning` never reach the provider. A failed row keeps its own error; siblings keep their answers.

`tools/judgment` is the one evaluator behind retrieval `clasifyContext` and `clasify`: prepared state, provider questions, optional captured image assets and capability options in; validated answers, model and usage out. It owns the provider client, request batching (all questions over one state in one request when the serialized group fits the headroom, else one request per question), response validation (`providers/classification/jev.rs`), retries (`octocode_github::Backoff`, `classify_status`, `read_bounded`), the process-wide endpoint gate (`providers/classification/gate.rs`, bounded by the runtime's constant classification concurrency, idle gates dropped past 64), and accounting. Its judgment cache is runtime-owned: a `cache::Store` (memory always; `<home>/tmp/judgment` on disk only under persistent storage; 256 entries, 8 MiB, 30-minute TTL) keyed by the credential `Partition`, model, state, questions, image bytes and capability options, with one `KeyedFlight` so identical concurrent evaluations make one request. Only complete validated evaluations are cached; a replay reports `replayed: true` and the original usage. The same authenticated transport exposes the provider model catalog, accounted separately from inference. Inline selection can return a per-call usage receipt through `extras:["stats"]` even when persistent stats are disabled. Image source capture reuses local access policy, GitHub content by resolved commit, HTTPS address checks, and the existing capture store; it sends bytes internally and returns only provenance and a saved inspection path. Retrieval compiles each atomic question through the contract's `classifierTemplate` into a Score question (`tools/judgment/select.rs`). `tools/selection` applies the core selection policy and partitions candidates into selected, excluded and unknown. It saves decisions in the `selection` plan, binds source identity, template, context and optional brief, and replays without new provider calls. Code search selects its sanitized provider page at the response boundary (`ToolOutput::selects_provider_page`); local search and both fetch tools select before their own pagination. Default output, recovery leads and the transient-provider fallback (`unless_transient`: the view shown unfiltered with an `unavailable` receipt) follow the [context selection rule](../../docs/OCTOCODE_TOOLS.md#context-selection).

`clasify operation:"sections"` shares those readers and the evaluator. `tools/source_sections` owns source-coordinate geometry used by localFetch selection and section planning; it reuses declaration parsing and UTF-8 line indexes. `tools/source_capture/resource` acquires one complete authorized text capture, preserving original bytes for planning before sanitization. The section executor retains only sanitized target/context state, shared questions, immutable source identity and per-question outcomes. It checks the retained-plan budget while adding units. Answers are emitted once their section is settled; pending work stays in coverage counts and `next.nextSectionPage`. Invalid provider answers fail their own cells while valid siblings survive, with one usage receipt per actual request.

The runtime owns section sessions separately from saved display pages. Work tokens bind access, source identity, questions, model and policy; a persistent transaction lock and fresh disk reads serialize cross-process progress. Active work is saved before inference so an interrupted request is reported as unassessed instead of automatically repeated. Local work revalidates the original locator, canonical path and snapshot; settled replay performs no inference. Section plans expire after 30 minutes and are bounded by eight entries, 32 MiB in memory and 128 MiB on disk. These are retained-state limits, not a process RSS ceiling. Memory-only CLI calls reject plans requiring continuation before inference; long-lived MCP sessions and persistent CLI storage can resume them. The user-facing contract is in [the classification guide](../../docs/OCTOCODE_CLASIFY.md#research-text-sections-across-resources).

Search and fetch never hand off to `clasify`, and emit no automatic reread lead of evidence they already returned.

## Ownership

Production imports point one way: `id`/`num` → `contracts` → `policy` → `config`/`security` → `execution` → `response` → `cache` → `providers` → `tools` → `runtime`. A lower module never imports a higher one (`tools/module_boundaries.rs` `LAYERS` enforces it); a callback the runtime gives a tool is a trait the lower layer defines.

| Module | Owns | Must not own |
|---|---|---|
| `id` / `num` | Tool identity (`ToolId`, error codes, continuation channels) and saturating integer conversions: leaves every layer imports | Anything above them |
| `config` | Native configuration acquisition and diagnostics | Tool behavior |
| `contracts` | Generated schemas/rules, defaults, validation, and canonical contract fingerprint | Provider or tool execution |
| `execution` | The request kernel: `ExecutionContext`/`ExecutionError`, cooperative cancellation, the runtime's code limits, and per-runtime tool availability | Anything from response, providers, tools or runtime |
| `runtime` | Admission, dispatch and ordered bulk orchestration (execution codes include `responseTooLarge` and `unroutedTool`). `runtime/error.rs` is the one projection of a rejected call: `tool_error_result` builds the `{error, details?, errorCode}` envelope and its one-line text for CLI JSON, the terminal, the warm server and MCP | CLI presentation or MCP SDK types |
| `policy` / `security` | Path, content, command, and secret policy; the one error scrubber (`scrub_error_text` / `scrub_error_payload`) shared by the CLI and MCP | Interface-specific behavior |
| `providers` / `cache` | Remote DTOs (GitHub, artifact registries, classification and its Jev wire validator), HTTP, retries, credentials, endpoint caches | CLI or MCP framing |
| `tools` | Public operations composed from shared runtime services and engine primitives | Independent config, auth, or response systems |
| `response` | Contract-checked rows, compression, pagination, continuations, sanitized rendering, and the result vocabulary tools write (`result`, `paging`, `page_windows`, `numbered`, `output`, `outcome`, `exit` classification) | Tool execution |
| `tools/lsp_search` + `runtime/domain_dispatch` | Runtime composition around the engine language-server pool (pool owned by the runtime engine, dispatched and prewarmed through `domain_dispatch`) | A second lifecycle implementation |
| `tools/<shared>` (the list and each module's purpose live in `tools/module_boundaries.rs` `SHARED_MODULES`) | Logic two or more tools use: for example `empty_walk` (why an empty local walk is empty and the minimal flags that lift it), `leads` (follow-up queries one tool offers into another), `declarations_cache`, `gh_shared`, `source_view` (the one view engine over fetched source text: localFetch and ghGetFileContent render through it), `source_capture` (its `SourceReader` trait is how clasify reads through the runtime's dispatcher without naming it) | Tool-specific behavior. The guard rejects every tool-to-tool and shared-to-tool edge. |
| `tools/ast_graph/store` | CLI-only persisted code graph (`npx octocode graph ingest` / `graph query`): immutable snapshots under `<workspace>/.octocode/graph/` | A public tool contract or MCP surface |
| `crates/cli` | Arguments, human output, and shell exits | Node or duplicated tools |
| `crates/cli/src/cli/mcp_server/` | The stdio MCP server: tool and instruction listing composed from the embedded presentation and runtime catalog (`listing.rs`), request dispatch into the runtime and era-specific response metadata (`handler.rs`), startup, signals and shutdown (`mod.rs`) | Tool behavior or contract text |
| `crates/cli/src/cli/serve/` | The warm server for read-only CLI tool calls: socket, runtimes per directory and environment on one language-server pool, and its limits ([README](README.md#warm-tool-server)) | Tool behavior |
| `crates/cli/src/cli/mcp_install.rs`, `mcp_manage.rs`, `mcp_clients.rs` | MCP client configuration writers and the client path catalogue | Tool execution |
| `crates/cli/src/cli/lsp_provision/` | Verified language-server downloads: allowlisted hosts, SHA-256 pinned assets, bounded time and size, streamed through an owner-only spool file under a per-target `flock` | Engine LSP discovery |
| `crates/io` | Primitives the crates above the engine share ([`crates/io/src/lib.rs`](crates/io/src/lib.rs) lists them) | Tool, provider, or contract logic |
| `crates/contract-types` | The generated tool wire types, nothing else (a crate of its own so a runtime edit does not re-check them) | Hand-written types, views, or trait impls |

The engine owns shared process containment and teardown (`process`), streaming
JSON size measurement (`json_size`), and redaction placeholder matching
(`security`). Runtime rows catch unwinding panics at `runtime/domain_dispatch`;
worker-thread isolation, sanitizer errors, and minifier fallback stay with their
owners. These independent threads and recovery policies cannot be replaced by
an outer row catch. Consumers call each engine operation at its defining module.

Six crates, and these Cargo edges only: `engine`, `io` and `contract-types` are
leaves; `github` → `io`, `contract-types`; `runtime` → `io`, `engine`, `github`,
`contract-types`; `cli` → `io`, `runtime`, `engine`, `github`, `contract-types`. `crates/runtime` is a pure `rlib`.
`crates/io` owns the primitives the crates above the engine share, among them
the one in-process memo outside the engine and the keyed single flight around it
([`crates/io/src/lib.rs`](crates/io/src/lib.rs) lists them and the engine's own caches).
`crates/io` also owns the provider-neutral HTTP rules every client shares
(`budget`: `RequestBudget`; `retry`: `Backoff`, `classify_status`,
`is_retryable_send`, `read_bounded`, `retry_after_delay`) and Gregorian day math
(`civil`). `crates/github` owns GitHub protocol and transport services, plus
`Partition` (an origin scoped by a digest of the exact credential; every cache,
limiter and rejection set derives from it) and `credential_host`, which artifact
HTTP and the classification transport reuse.
GitHub content reads take one flight per `(partition, commit, path)`, and a cached
body at a resolved commit is served without revalidation; a movable ref stays
fresh through its own memoized resolution. Runtime keeps configuration,
credential selection, cache policy, and generated tool contracts. `crates/cli`
owns the `octocode` binary (CLI and MCP server). All six crates inherit one workspace version and remain
unpublished Cargo packages.

In-memory page and scan memos (`tools/page_memo.rs`, `octocode_io::memo`) are
process-global statics, bounded by TTL, entry, and byte limits, and keyed by
snapshot and `PathPolicy` identity; an `octocode serve` process shares them
across all of its runtimes. On-disk runtime caches live under `<home>/tmp/`
only when storage is persistent: `response` (tool response cache),
`response-pages` (immutable settled output pages),
`artifact` (registry responses), `judgment` (classification answers),
`section-assessments` (frozen section work plans),
`ratelimit` (cross-process GitHub limiter facts), `capture/url` (URL source
captures), and `clone` / `clone-tmp` (`ghCloneRepo`). The response, artifact
and judgment stores evict by entry class and byte budget themselves;
`runtime/maintenance.rs` sweeps the rest by age once a day (rate-limit facts,
URL captures at their published retention). Clone entries are evicted only
through `ghCloneRepo`, which checks their locks and local edits.

Saved output pages use the shared cache store with 120-second retention, at most
eight entries, a 32 MiB memory budget and a 128 MiB disk budget. Their compact
continuations carry the response handle and page window, without repeating input
state or questions. The handle binds tool, surface, credentials, policy, workspace
and contract. A per-request trace records the GitHub credential partitions used
by direct tools and indirect captures. Local auth inspection must match that
partition before retention and replay; it never refreshes credentials. Reading
the handle never reacquires source or runs inference; an expired
handle fails explicitly. Memory storage keeps these pages within one runtime;
persistent storage permits another CLI process to read them. A memory-only CLI
call returns the complete result with a warning, since a new process cannot
read its memory. If the complete
result cannot be retained, the runtime returns it intact with an oversized
disclosure instead of offering an unreachable page.

The engine's `scan_budget` holds the time and walk budget of an unbounded scan (no caller `maxFiles`): it stops at a chunk boundary past the budget and returns the walk-order index to resume from, and `lsp/module_path` anchors bundled language-server lookup at `current_exe()`, the one native binary.

`crates/engine` is consumed as a Rust library with its default grammar set. PCRE2 is unconditional, structural matching is implemented by the engine’s `structural/octo` modules over the shared Tree-sitter registry, and the default build includes every grammar. It exposes reusable algorithms, not public policy, and has no host-interface surface. Engine errors carry a `reason` and an `ErrorKind`.

Tension: the engine is described as algorithms, not policy, yet it owns the language-server pool and its process IO (`lsp/pool.rs`, `lsp/client.rs`). Those stay in the engine so the CLI daemon and the MCP server share one pool; moving them out is a design decision, not a cleanup.

AST declaration extraction indexes local export aliases by symbol name in the engine
and polls cancellation during outline construction. Runtime owns a bounded cache of
private declaration JSON for single-file symbol requests (128 entries, 32 MiB,
cache class `Immutable`: an entry never expires, because its key holds the
source-content hash; size and entry caps evict it). Each request validates workspace access and reads current source
before lookup; the key includes canonical path, parser choice, and a source-content
hash. Source edits invalidate pagination snapshots, and policy shaping runs on every
request. Directory extraction is not cached. Parsing occurs outside the cache lock;
failed or cancelled extraction is never retained.

## GitHub authentication

`providers/github/auth` owns one lazy credential-selection flow shared by tool
requests and CLI auth inspection. Config owns environment-token precedence and
protected keys through `config-contract.json`; Node interfaces pass their
environment through without discovering or injecting credentials.

The resolver checks an explicit override, then the configured host's environment
token, encrypted home credentials, existing OS credentials, and finally host-scoped
`gh auth token`. Stored
credentials retain their username and expiry metadata. Request mode refreshes
expired stored credentials under the existing cross-process lock and rereads
storage after acquiring it; inspection mode never refreshes. A deleted credential
is not reused. A usable `gh` fallback keeps its own source identity and never
enters the stored-token refresh path. CLI source labels remain `env`, `platform`,
`gh-cli`, and `none`; home credentials add `octocode-storage`.

Discovery uses the runtime's explicit environment and adds common Homebrew paths
after the supplied PATH. Environment tokens are removed from the `gh` subprocess
because the resolver has already applied their host scope. Its output is bounded,
and timeout/cancellation paths kill and reap the child. Discovery and refresh
share the request deadline; cancellation and timeout do not start another fallback.
OS keychain calls run on blocking workers and are awaited before checking the
budget again: an OS keychain prompt cannot be forcibly cancelled by the resolver.
The selected credential is pinned before transport/cache work begins. Local
tools and catalog inspection perform no credential discovery.

`CredentialStore` takes the resolved Octocode home from config. New logins and
N-API credential saves use main-compatible AES-256-GCM home files; existing native
OS-store credentials remain a fallback. Refresh persists to its original source.
Home updates lock the document and atomically replace it, preserving other hosts.
A refresh cannot overwrite a newer home login or recreate a deleted credential.
Forced login replaces credentials only after successful authorization and saving.
Logout removes the host from both stores and leaves environment and gh auth alone.

Keep discovery and OAuth in native: both the standalone binary and N-API runtime
need the same lazy, host-scoped selection, refresh locks, deadlines, and provenance.
Moving those effects into Node config would duplicate the standalone path and
flatten credentials into environment tokens, losing refresh metadata and source.
Config remains the shared home/dotenv policy owner; core contracts are unchanged.

## Language capability ownership

`crates/engine/src/signatures/languages.rs` is the sole native grammar inventory. Structural search/rewrite, signatures, graph facts, syntax inspection, directory language filters, and LSP grammar adapters derive from that registry; built-in language-server routing comes from a separate server table (`lsp/config/catalog.rs`). Counts, grammar features, and per-language coverage: [SUPPORTED_LANGUAGES_AND_FEATURES.md](docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md). An astSearch rule is a JSON object, decoded once into the engine's typed rule; YAML files are read as source text only.

`engine::lsp::managed` owns the pinned server manifest and the verified install layout `<home>/lsp/<server>/<tag>/<bin>` with a `.ok` marker; the CLI only downloads and installs. Route resolution order is in [LSP_SERVER_LIFECYCLE.md](docs/engine/LSP_SERVER_LIFECYCLE.md#resolution). Engine LSP discovery reads no `OCTOCODE_*` process environment; the runtime passes resolved options.

Generic text search, reads, minification, file recognition, GitHub/history operations, artifact registries, and trusted custom LSP configuration do not consult the grammar allowlist. Syntax graph relations remain candidate evidence; callers use explicit LSP operations for semantic proof.

## Local walk policy

Every local tool walks under one policy owned by `engine::search::walk` and one glob dialect owned by `engine::search::glob`:

- Ignore rules are standard gitignore semantics as the `ignore` crate implements them: `.gitignore` (inside a git repository only), `.ignore`, the global gitignore, `.git/info/exclude`, and the ignore files above the walked root. `IgnorePolicy { include_ignored, include_hidden }` comes from the request's `includeIgnored`/`includeHidden`; `includeIgnored:true` lifts every source at once. `walk_builder` applies it inside a walk; `IgnoreRules` decides one path the same way and names the deciding rule (`why`), for empty-result hints.
- Globs (`include`, `exclude`, `names`, `languageGlobs`) are gitignore globs: `*` never crosses `/`, a glob without `/` matches a name at any depth, one with `/` the path below the root. An invalid glob is always an `invalidGlob` error.
- A walk is cancelled by one `stop` function (`ErrorKind::Cancelled`); directory names are pruned by one `Prune` set.
- `engine::text::read_source` is the one bounded read (non-blocking open, size from the opened handle, capped read) and `engine::text::sniff` the one binary definition (a NUL, or dense C0 controls, in the first 8 KiB).

`fs_query` (structureSearch, graph scans) keeps its own name-ordered recursion: its policy callback prunes subtrees and probes refuse oversized directories before listing them, which the `ignore` walker's `'static` entry filter cannot express. It applies the same `IgnoreRules`, `Prune`, and globs.

## Safety and lifecycle invariants

- Native runtime absence fails closed at every Node interface.
- Requests are admitted before asynchronous work starts and own cleanup through completion.
- Cancellation, worker handles, HTTP clients, caches, and LSP clients are runtime-owned resources.
- Credentials are pinned per request and never copied into public responses.
- Every completed row is checked against its generated tool output contract.
- Pagination and continuations retain the query identity needed to resume safely.
- Retrieval context selection withholds excluded and unknown bodies by default, and preserves successful sibling decisions after failures ([context selection](../../docs/OCTOCODE_TOOLS.md#context-selection)). Local recovery is snapshot-bound; GitHub file recovery is commit-pinned. GitHub search recovery reruns the live provider page and can drift. Its candidate continuation binds the complete captured page digest and rejects drift before further classification.
- `localFetch` and `ghGetFileContent` share extraction: literal and regex matches use original byte coordinates and include every source line touched by a multiline match. Case-insensitive Unicode normalization never supplies offsets for slicing original content. `regex:"rust"` is the Rust regex engine `localSearch` also runs, so one pattern has one verdict and reason in both tools at any file size; backreferences and lookaround need `regex:"pcre2"`.
- Structural rewrite uses embedded engine primitives while the tool layer retains locks, hashes, path policy, selection, postconditions, transactions, and recovery.
- Public rewrite paths (rows, patch headers, `expectedHashes` keys) are workspace-relative like every local tool's rows; guarded apply resolves relative keys against the workspace root and also accepts absolute keys. Match ids stay keyed on the scanned-directory path, and the snapshot carries a contract version, so a preview from an older path form fails closed with `staleSnapshot` and `next.restart`.
- A poisoned lock is recovered with `PoisonError::into_inner`, never skipped: a skip silently drops work, and on coordination state it strands waiters. `crates/runtime/tests/lock_poison_policy.rs` enforces this across the crates, with a reason for each allowed exception.
- No external search, AST, rewrite, or provider executable is used. Intentional subprocesses are limited to system Git cloning, configured language servers, and supported credential discovery.

## Contract generation

`@octocodeai/octocode-core` authors the tool contracts (Zod). `@octocodeai/config` is the only generator: `yarn contracts:regen` (repo root) refreshes core and writes `packages/octocode-config/contract/` — the enforcement contract JSON, parity fixtures, provenance, and generated Rust wire types. Native keeps no copy. `build.rs` embeds those files in place (`contracts::generated`), reruns when they change, and refuses to build when provenance or `tool_types.rs` names a different fingerprint than the contract. A test recomputes the embedded bytes' SHA-256 against provenance, so a hand edit fails without any native-side pin to update.

Tool wire types come from the same run: the `octocode-contract-types` crate (`crates/contract-types`) `include!`s `contract/tool_types.rs` (typify, from the bundled JSON Schema that also produces the TypeScript types), trimmed to the generated types native source names and what they reach, so native compiles only those. Naming a generated type the file lacks fails to compile until `yarn contracts:regen`; `check:tool-contract` fails when the names native uses changed. Ordinary tools parse their validated rows directly into generated `<Tool>Query` types (meta fields included) and build continuations from them. Tool modules add accessors for engine integer types as plain functions taking the query (`local_fetch_query::start_line(&query)`: Rust allows no `impl` block on another crate's type), but never a second serde wire type. `tests/contract_field_effects.rs` fails only when the contract gains a field or discriminator value with no declared native effect in `field-effect-coverage.json`.

Contract annotations native alone reads stay out of every agent-facing schema. One is `x-listing` (core `listing()`): what a listing query field does to the walk (`name`, `metadata`, `prune`, `scope`). `build.rs` renders it as `contracts::listing_roles`, and structureSearch derives its filter lists, its filtered-listing test and the walk identity a cursor continues from that table. A role native does not know fails the build. A field with a role that the walk does not apply fails `structure_search::tests::roles`.

`config` reports the config and dotenv paths, key names, and config warnings. `config set` and `config unset` use `config/edit.rs` for locked atomic edits, sharing the dotenv parser and generated protected-key policy. They preserve unrelated lines and never return values.

Configuration policy is independently owned by `@octocodeai/config` in `config-contract.json`. The config generator validates that declaration against its meta-schema (Ajv) and commits it pre-validated. Native `build.rs` does not re-run meta-schema validation; it renders Rust config structs, defaults, environment policy, and generic resolver/validator metadata into `OUT_DIR` and fails closed on any missing or mistyped field it reads. Generated files are not hand edited.

## Bundled skills

`crates/cli/src/cli/skill/` owns the `SKILL.md` name line, the platform registry, installation,
freshness checks, repair and removal. The Node launcher delegates these commands
to the native executable. Runtime config owns home and credential resolution.

`crates/cli/build.rs` runs `scripts/embed-skills.mjs`, which uses the Node package's
`stage-skills.mjs` filter to stage canonical `skills/` files into Cargo's output
directory. Rust embeds those bytes and executable permissions. Cargo watches the
source tree and staging scripts, so a changed workflow is included on rebuild.
The script also decodes each skill's `description`, which must be one frontmatter line
holding a JSON string, and embeds it as a `&'static str`; `skills:check` enforces that
form for the public skills. At runtime Octocode reads no YAML: `skill install --add`
takes `name` from its single `name:` line (bare or double-quoted), and any other form
asks for a name override.
Node is a build prerequisite; no Node process or extraction cache is needed to
list, inspect, install, check or remove skills from a shipped binary.

Installation copies into `$OCTOCODE_HOME/skills/<name>` and links selected global
or project host directories to that durable copy. Windows link mode uses directory
junctions. Replacement stages beside the target, retains a backup until publication
succeeds, and restores it on failure. `--upgrade` updates copy destinations only
when they still match the previous canonical tree; edited copies require `--force`.
Dry runs validate and report conflicts without writing. Freshness checks compare
paths, sizes and bytes, reuse comparisons within a check, and leave external
development links alone. The CLI tests run with an empty executable search path;
the migration fixtures preserve installer reports and filesystem behavior.

## Build modes

`scripts/build-native.cjs` builds `octocode-cli` (all its binaries) in one Cargo
invocation with the committed lockfile.

- The binary contains the full CLI, the MCP server and the runtime. The crate declares no default Cargo features.
- Every build stages the host binary into its platform package so the local launchers execute the rebuilt runtime. Staging atomically replaces the inode — never an in-place overwrite, which can crash a running MCP server or leave a stale macOS code-signature cache.
- Each platform package contains the optimized native binary only; there is no addon and no root entrypoint that loads one.
- Host-platform staging and platform verification run the binary in a subprocess before release acceptance.
- Release acceptance exercises the direct native CLI, the built Node launchers, and real stdio MCP calls (`octocode mcp` directly and through `octocode-mcp`).

Rust crate extraction does not create new npm packages. Build commands use the
committed lockfile; release preparation synchronizes versions before compilation,
and prepublish checks versions without rewriting manifests or resolving dependencies.

Build notes:
- `crates/runtime/build.rs` resolves `packages/octocode-config` from the manifest directory, so the crate builds only inside this monorepo. That is fine while `publish = false`.
- The optional dependencies in `crates/runtime/Cargo.toml` exist to unify feature sets for development builds.
