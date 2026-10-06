# Security

Octocode enforces security inside the native Rust runtime shared by CLI and MCP. Interfaces cannot bypass it and there is no TypeScript fallback.

## Request path

```text
contract validation
  → configuration and capability admission
  → input bounds and dangerous-key checks
  → provider or local path authorization
  → bounded, cancellable execution
  → content sanitization
  → output-contract validation and safe rendering
```

A sanitization failure discards the unsafe value and returns a typed error. Bulk rows are isolated, but each successful row passes the same checks.

## Input validation

Generated `@octocodeai/octocode-core` schemas reject unknown fields and invalid operation combinations. Native validation additionally rejects:

- strings longer than 10,000 UTF-16 code units;
- arrays longer than 100 entries;
- object nesting deeper than 20 levels;
- `__proto__`, `constructor`, and `prototype` keys;
- invalid paths, cursor identities, snapshots, hashes, and numeric bounds.

Credentials are acquired after request admission and pinned for the request lifetime. They are never accepted through ordinary tool query fields.

## Content sanitization

`packages/octocode-native/crates/engine/src/security/` owns the canonical ordered secret-pattern set and native scanner. It covers cloud, AI-provider, version-control, package-registry, database, payment, communications, private-key, bearer-token, and connection-string formats. File-context patterns activate only for matching path classes to reduce false positives.

The runtime scans untrusted provider and filesystem content before rendering it. Detected values are replaced with typed redaction markers and accompanied by warnings. Oversized values are replaced wholesale rather than partially exposed. A final recursive pass sanitizes nested strings while preserving executable continuation and location structures. Email masking in GitHub output is opt-in: `--redact-emails`, `OCTOCODE_REDACT_EMAILS=true`, or `output.redactEmails`.

The Rust implementation is the only production scanner. `patterns.rs` is its source of truth; a test-only complete regex set verifies that the optimized literal prescan does not lose matches.

### Classification egress

`clasify` sends data to the configured classification provider (`OCTOCODE_CLASSIFICATION_API_HOST`, default Jev); nothing leaves when no key is set. Each request carries:
- the captured evidence, after the same output sanitization as a direct tool call, including a search page's absolute `base`;
- the matrix `mainGoal` and `reasoning`, when the caller set them;
- the question text;
- a `read` descriptor naming the tool and its query (search text, paths, repositories).

The `read` query passes the input security policy before it is sent: a query the policy rejects is dropped, and secrets in it are redacted. Paging tokens (`snapshot`) are removed. Treat everything in a clasify matrix as disclosed to that provider.

## Filesystem policy

Every local operation resolves through `packages/octocode-native/crates/runtime/src/policy/path.rs`.

- Allowed roots are the workspace root (`WORKSPACE_ROOT` / `local.workspaceRoot`, else the process cwd), `ALLOWED_PATHS` / `local.allowedPaths`, and `OCTOCODE_HOME`. The OS home directory is **not** allowed unless one of these covers it (`runtime/src/runtime/engine.rs`).
- Relative paths resolve against the process working directory, not `WORKSPACE_ROOT`; pass absolute paths.
- Relative traversal and paths outside allowed roots are denied (`outsideAllowedRoots` from every local tool).
- System directories such as `/etc` stay denied even when listed in `ALLOWED_PATHS`.
- Sensitive names and directories are pruned during discovery and denied again before reads.
- Symlinks are revalidated against their canonical targets; escaped descendants are rejected.
- File type, size, mutation, and snapshot checks happen before evidence is returned.

Sensitive classes include environment files, private keys and certificates, credential stores, cloud configuration, shell history, browser login stores, infrastructure state, wallets, and application secret files. Denial messages name the path as requested or relative to `~`, and outside-root denials list the allowed roots; with `debug: true`, a `localFetch` row's `resolvedPath` carries the resolved path, which can be absolute.

Set `ENABLE_LOCAL=false` to disable local tools. `OCTOCODE_BETA=true` (or `local.beta:true`, shell or
home config only), default off, gates `astTopology` and `astRewrite`. Both are
CLI-only (MCP never exposes them); for `astRewrite` the gate permits both preview and its hash-guarded mutation path.

## Structural rewrite safety

`astRewrite` uses embedded engine primitives; it does not launch `ast-grep` or another rewrite executable.

Preview is read-only. Apply requires the native configuration gate and exact preview identities. The runtime retains:

- canonical-root and path-policy checks;
- per-root exclusion locks;
- before/after hashes and unchanged-source guards;
- explicit match selection;
- postcondition checks;
- staged multi-file transactions;
- crash journals and rollback;
- cancellation checks between bounded steps.

Public file paths are relative to the preview root. A guarded human apply resolves them against that root before revalidation.

## External processes

Search, AST analysis, rewrite, providers, response shaping, and bulk orchestration are embedded Rust. External processes are limited to capabilities that intentionally require them:

- system Git for `ghCloneRepo`, with HTTPS-only remotes, bounded arguments/output, process-group cancellation, staged publication, and no token in argv;
- configured language servers, launched without a shell and owned by the native LSP pool;
- `gh auth token` as the final supported GitHub credential source;
- the packaged bounded regex worker for JavaScript-regex features that Rust regex intentionally does not implement.

Language-server provisioning accepts only pinned assets from allowed HTTPS hosts, verifies SHA-256 before extraction, uses per-target locks, and publishes atomically.

## GitHub credentials

Resolution order (environment → encrypted Octocode login → OS credential store → `gh auth token`), login, refresh, and logout are documented in [AUTHENTICATION.md](AUTHENTICATION.md). The security properties:

- Tokens are never accepted through tool query fields, and an environment token is attached only to the host of the configured `GITHUB_API_URL`, which only the shell or home config can set.
- `<OCTOCODE_HOME>/credentials.json` uses AES-256-GCM (16-byte IV, authentication tag, ciphertext) with the key in `.key`. Both are mode `0600` on Unix; a newly created home directory is `0700`. Writes are locked and atomically replaced; corrupt or unauthenticated files are rejected without being overwritten; symlinked credential files and Unix hard links are rejected. The adjacent key does not protect against someone who can read both files.
- `gh auth token` runs without Octocode's token variables in its environment, with a 5-second bound.
- `.octocoderc` never supplies GitHub tokens, and bootstrap variables such as `PATH`, `HOME` and `NODE_OPTIONS` are blocked in every `.env` file ([rules](CONFIGURATION.md#env--environment-fallback)).

GitHub endpoint, credential, session, and cache identities are partitioned. Pagination redirects must remain same-origin. Retries and rate-limit delays are bounded, response bodies are capped, and GraphQL partial failures remain explicit.

## Cancellation and lifecycle

The runtime admits requests through bounded queues. Cancellation and timeout propagate through provider requests, filesystem work, regex workers, Git, and LSP. Dropped callers release admission and owned resources; runtime shutdown joins workers and terminates pooled processes. Partial or capped evidence is never labeled complete without an executable continuation or explicit terminal diagnostic.

## Reporting vulnerabilities

Open a private GitHub security advisory for the Octocode repository rather than a public issue.
