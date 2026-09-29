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

The runtime scans untrusted provider and filesystem content before rendering it. Detected values are replaced with typed redaction markers and accompanied by warnings. Oversized values are replaced wholesale rather than partially exposed. A final recursive pass sanitizes nested strings while preserving executable continuation and location structures.

The Rust implementation is the only production scanner. `patterns.rs` is its source of truth; a test-only complete regex set verifies that the optimized literal prescan does not lose matches.

### Classification egress

`clasify` sends data to the configured classification provider (`OCTOCODE_CLASSIFICATION_API_HOST`, default Jev); nothing leaves when no key is set. Each request carries:
- the captured evidence, after the same output sanitization as a direct tool call, including a search page's absolute `base`;
- the matrix `goal` and `reasoning`;
- the question text;
- a `read` descriptor naming the tool and its query (search text, paths, repositories).

The `read` query passes the input security policy before it is sent: a query the policy rejects is dropped, and secrets in it are redacted. Paging tokens (`snapshot`, `cursor`) are removed. Treat everything in a clasify matrix as disclosed to that provider.

## Filesystem policy

Every local operation resolves through `packages/octocode-native/crates/runtime/src/policy/path.rs`.

- The OS home directory is allowed by default.
- `WORKSPACE_ROOT` / `local.workspaceRoot` and `ALLOWED_PATHS` / `local.allowedPaths` add explicit roots.
- Relative traversal and paths outside allowed roots are denied.
- Sensitive names and directories are pruned during discovery and denied again before reads.
- Symlinks are revalidated against their canonical targets; escaped descendants are rejected.
- File type, size, mutation, and snapshot checks happen before evidence is returned.

Sensitive classes include environment files, private keys and certificates, credential stores, cloud configuration, shell history, browser login stores, infrastructure state, wallets, and application secret files. Denied errors use safe relative paths instead of echoing private absolute paths.

Set `ENABLE_LOCAL=false` to disable local tools. `astRewrite` is a CLI-only beta feature (MCP never exposes it)
gated solely by `OCTOCODE_BETA=true` (or `local.beta:true`), default off, which
permits both preview and its hash-guarded mutation path.

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

Credentials first choose the highest-priority source: process environment → workspace `.octocode/.env` → global Octocode `.env`. Within that source, alias order is:

1. `OCTOCODE_TOKEN`
2. `GH_TOKEN`
3. `GITHUB_TOKEN`
4. `GITHUB_PERSONAL_ACCESS_TOKEN`

If no environment credential is available, resolution continues through encrypted credentials in `OCTOCODE_HOME`, the operating-system credential store (existing native logins), then host-scoped `gh auth token`.

CLI and MCP load both `.env` files. A workspace alias overrides a global canonical key; `.octocoderc` does not supply GitHub tokens. Missing or blank file values allow fallback. Bootstrap variables such as `PATH`, `HOME`, and `NODE_OPTIONS` remain blocked in both files; product settings such as `GITHUB_API_URL` follow the shared precedence. See [configuration rules](CONFIGURATION.md#env--environment-fallback). Native login stores OAuth credentials in `<OCTOCODE_HOME>/credentials.json` using main’s AES-256-GCM format (16-byte IV, authentication tag, ciphertext), with the key in `.key`. The files use mode `0600` on Unix; newly created home directories use `0700`. Writes are locked and atomically replaced; corrupt or unauthenticated files are rejected without overwriting them. Symlink credential files and Unix hard links are rejected. The adjacent key means this does not protect against someone who can read both files. Existing OS-store credentials remain readable. Logout deletes the selected host from both Octocode stores; it does not change environment variables or GitHub CLI login.

GitHub endpoint, credential, session, and cache identities are partitioned. Pagination redirects must remain same-origin. Retries and rate-limit delays are bounded, response bodies are capped, and GraphQL partial failures remain explicit.

## Cancellation and lifecycle

The runtime admits requests through bounded queues. Cancellation and timeout propagate through provider requests, filesystem work, regex workers, Git, and LSP. Dropped callers release admission and owned resources; runtime shutdown joins workers and terminates pooled processes. Partial or capped evidence is never labeled complete without an executable continuation or explicit terminal diagnostic.

## Reporting vulnerabilities

Open a private GitHub security advisory for the Octocode repository rather than a public issue.
