# Security

The native Rust runtime shared by CLI and MCP enforces every rule on this page. Interfaces cannot bypass it, and there is no TypeScript fallback.

```text
contract validation → configuration and capability admission → input bounds and dangerous-key checks
  → provider or local path authorization → bounded, cancellable execution → content sanitization
  → output-contract validation and safe rendering
```

A sanitization failure discards the unsafe value and returns a typed error. Bulk rows are isolated, and each successful row passes the same checks.

## Input validation

The generated `@octocodeai/octocode-core` schemas reject unknown fields and invalid operation combinations. Native validation also rejects strings over 10,000 UTF-16 code units, arrays over 100 entries, nesting deeper than 20 levels, `__proto__`/`constructor`/`prototype` keys, and invalid paths, cursor identities, snapshots, hashes, and numeric bounds.

Credentials are acquired after request admission and pinned for the request lifetime. Tool query fields never accept credentials.

## Content sanitization

One ordered secret-pattern set and native scanner cover cloud, AI-provider, version-control, package-registry, database, payment, communications, private-key, bearer-token, and connection-string formats. File-context patterns apply only to matching path classes, to reduce false positives.

The runtime scans untrusted provider and filesystem content before rendering. Detected values become typed redaction markers with warnings. Oversized values are replaced whole, never partly exposed. A final recursive pass sanitizes nested strings and keeps continuation and location structures executable. Email masking in GitHub output is opt-in: `--redact-emails`, `OCTOCODE_REDACT_EMAILS=true`, or `output.redactEmails`.

### Classification egress

`clasify` sends data to the configured provider (`OCTOCODE_CLASSIFICATION_API_HOST`, default Jev); nothing leaves without a key. Each request carries:

- the captured evidence, sanitized like a direct tool call, including a search page's absolute `base`;
- the matrix `mainGoal` and `reasoning`, when set;
- the question text;
- a `read` descriptor naming the tool and its query (search text, paths, repositories).

The `read` query passes the input security policy first: a rejected query is dropped, secrets are redacted, and paging tokens (`snapshot`) are removed. Treat everything in a clasify matrix as disclosed to that provider.

## Filesystem policy

- Allowed roots are the workspace root (`WORKSPACE_ROOT` / `local.workspaceRoot`, else the process cwd), `ALLOWED_PATHS` / `local.allowedPaths`, and `OCTOCODE_HOME`. The OS home is **not** allowed unless one of these covers it.
- Relative paths resolve against the process working directory, not `WORKSPACE_ROOT`; pass absolute paths.
- Relative traversal and paths outside the allowed roots fail with `outsideAllowedRoots` on every local tool.
- System directories such as `/etc` stay denied even when listed in `ALLOWED_PATHS`.
- Sensitive names and directories are pruned during discovery and denied again before reads: environment files, private keys and certificates, credential stores, cloud configuration, shell history, browser login stores, infrastructure state, wallets, and application secret files.
- Symlinks are revalidated against their canonical targets; escaped descendants are rejected.
- File type, size, mutation, and snapshot checks run before evidence is returned.

Denial messages name the path as requested or relative to `~`; outside-root denials list the allowed roots. With `debug: true`, a `localFetch` row's `resolvedPath` holds the resolved path, which can be absolute.

`OCTOCODE_ENABLE_LOCAL=false` disables local tools. `OCTOCODE_BETA=true` (or `local.beta: true`; shell or home config only; default off) gates the CLI-only `astTopology` and `astRewrite`, including the `astRewrite` hash-guarded apply path. MCP never exposes them.

## Structural rewrite safety

`astRewrite` uses embedded engine primitives; it launches no rewrite executable. Preview is read-only. Apply needs the configuration gate and exact preview identities, and keeps canonical-root and path-policy checks, per-root exclusion locks, before/after hashes and unchanged-source guards, explicit match selection, postcondition checks, staged multi-file transactions, crash journals with rollback, and cancellation checks between bounded steps. Public file paths are relative to the preview root; an apply resolves them against that root before revalidation.

## External processes

Search, AST, rewrite, providers, response shaping, and bulk orchestration are embedded Rust. External processes run only for:

- system Git for `ghCloneRepo`: HTTPS-only remotes, bounded arguments and output, process-group cancellation, staged publication, no token in argv;
- configured language servers, launched without a shell and owned by the native LSP pool;
- `gh auth token`, the last GitHub credential source;
- the packaged bounded regex worker for JavaScript-regex features that Rust regex does not implement.

Language-server provisioning accepts only pinned assets from allowed HTTPS hosts, verifies SHA-256 before extraction, uses per-target locks, and publishes atomically.

## GitHub credentials

Resolution, login, refresh, and logout: [AUTHENTICATION.md](AUTHENTICATION.md). Security properties:

- Tool query fields never accept tokens. An env token goes only to the host of `GITHUB_API_URL`, which only the shell or home config can set.
- `credentials.json` uses AES-256-GCM (16-byte IV, authentication tag, ciphertext) with the key in `.key`. Both are `0600` on Unix; a newly created home directory is `0700`. Writes are locked and atomic. Corrupt or unauthenticated files are rejected and not overwritten; symlinked credential files and Unix hard links are rejected. The adjacent key does not protect against someone who can read both files.
- `gh auth token` runs without Octocode's token variables, bounded to 5 seconds.
- `.octocoderc` never supplies GitHub tokens, and every `.env` file blocks bootstrap variables such as `PATH`, `HOME`, and `NODE_OPTIONS` ([rules](CONFIGURATION.md#env--environment-fallback)).

GitHub endpoint, credential, session, and cache identities are partitioned. Pagination redirects must stay same-origin. Retries and rate-limit delays are bounded, response bodies are capped, and GraphQL partial failures stay explicit.

## Cancellation and lifecycle

Requests enter through bounded queues. Cancellation and timeouts propagate through provider requests, filesystem work, regex workers, Git, and LSP. A dropped caller releases its admission and resources; shutdown joins workers and ends pooled processes. Partial or capped evidence is never labeled complete without an executable continuation or an explicit terminal diagnostic.

## Reporting vulnerabilities

Open a private GitHub security advisory for the Octocode repository, not a public issue.
