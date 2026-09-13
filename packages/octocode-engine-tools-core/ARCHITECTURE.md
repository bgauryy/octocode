# Native tool runtime architecture

The target runtime has one Rust execution path:

```text
native octo CLI ──────────────┐
                             ├─ Rust runtime / policy / tools
Node MCP → optional NAPI ─────┘           ├─ native config
                                         ├─ providers / registry adapters
                                         └─ portable octocode-engine primitives
```

The CLI never loads NAPI or runs JavaScript. The Node MCP interface owns protocol
framing and generated registration only. It uses the same Rust runtime; no
provider, config, security or LSP implementation lives in JavaScript.

## Current state

The native CLI and optional addon execute localFetch through the same Rust
runtime. Config resolution, generated validation, path/content policy and
request lifecycle are native. The first CLI/MCP read suites pass, while expanded
parity, whole-response budgets, other tools and performance remain open. The
Node candidate entry performs SDK registration and forwarding only.

The crate builds binaries without addon features and builds the addon as a
library with `napi-addon`. Advanced ECMAScript patterns use a separately bounded
Rust helper; its integration and platform resource checks are still in progress.
The helper is counted in process-tree latency/CPU/memory, never hidden as free
work. Darwin RSS enforcement is sampled rather than a hard allocation ceiling.

## Ownership

| Module | Owns | May not depend on |
|---|---|---|
| config | Home/env acquisition, parsing, validation, resolution and diagnostics | Engine, providers, tools, NAPI |
| contracts | Generated canonical schemas/rules and native input validation | Tool execution or Node validators |
| runtime/policy/cache | Dispatch, request context, security, cancellation and resource limits | CLI formatting or MCP SDK types |
| tools | Canonical operations using shared services | Independent config/auth/security implementations |
| providers/registries | Remote DTOs, HTTP/retry/cache/credential policy | CLI or MCP framing |
| lsp | Native pool, context, discovery, provisioning and semantic operations | TypeScript engine wrappers |
| adapter_napi | Host conversion, runtime handle and lifecycle | A separate execution implementation |
| CLI | Arguments, human output and shell exits | Node runtime or NAPI |

Existing engine Rust algorithms remain in their owning crate with optional NAPI
bindings. Their portable APIs accept resolved options; they do not import this
higher-level runtime. The public tool core remains the contract authoring owner;
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
