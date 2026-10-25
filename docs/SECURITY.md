# Security

The native Rust runtime shared by CLI and MCP enforces every rule on this page. Interfaces cannot bypass it, and there is no TypeScript fallback.

```text
contract validation → configuration and capability admission → input bounds and dangerous-key checks
  → provider or local path authorization → bounded, cancellable execution → content sanitization
  → output-contract validation and safe rendering
```

A sanitization failure discards the unsafe value and returns a typed error. Bulk rows are isolated, and each successful row passes the same checks.

## Input validation

The generated `@octocodeai/octocode-core` schemas reject unknown fields and invalid operation combinations. Native validation also rejects ordinary parameter strings over 10,000 UTF-16 code units, arrays over 100 entries, nesting deeper than 20 levels, `__proto__`/`constructor`/`prototype` keys, and invalid paths, cursor identities, snapshots, hashes, and numeric bounds.

Clasify literal `content` uses its declared serialized-content limit instead of the ordinary string limit; nested values remain scanned and bounded. Credential-looking object keys are rejected without echoing or renaming them.

Public calls accept `{queries:[...]}` with optional `responseChars`, or an issued `{cursor}`. A cursor saves the complete normalized request and private progress, bound to its target tool, access identity, workspace, configuration, and contract. Caller-supplied private progress is rejected; invalid or expired handles never restart the original work.

Credentials are acquired after request admission and pinned for the request lifetime. Query locators and controls never accept credentials; literal classification content is sanitized before inference.

## Content sanitization

One ordered secret-pattern set and native scanner cover cloud, AI-provider, version-control, package-registry, database, payment, communications, private-key, bearer-token, and connection-string formats (300+ patterns). File-context patterns apply only to matching path classes, to reduce false positives.

The runtime scans untrusted provider and filesystem content before rendering. Detected values become typed redaction markers with warnings. Oversized values are replaced whole, never partly exposed. A final recursive pass sanitizes nested strings and keeps continuation and location structures executable. Email masking in GitHub output is opt-in: `OCTOCODE_REDACT_EMAILS=true` or `output.redactEmails`.

### Classification egress

`clasify` and `classification` send text evidence to the configured provider (`OCTOCODE_CLASSIFICATION_API_HOST`, default Jev); classification sends nothing without a key. Context selection sends the classifier predicates, optional `classification.context`, and authorized candidate evidence: paths with returned matches, raw text windows with source line numbers, or the selected snippets of result rows. It does not hydrate unseen bodies or widen requested fields, sections, or ranges. Public `clasify` can also send explicitly referenced images. Each evaluation request carries:

- the model and the caller's keyed questions, with optional task `context` and secret-bearing values sanitized;
- one state: the caller's literal `content`, or the complete selected source captured through the authorized reader and sanitized like a direct tool call;
- optional explicitly referenced PNG/JPEG assets, captured internally and sent as their original bytes. Image pixels and metadata are not text-scanned or redacted.

Treat questions and selected evidence as disclosed to that provider. Ordinary calls without `classification` send no evidence. When public `clasify` is configured and allowed, host startup sends an authenticated model-catalog ping; neither that ping nor the model catalog operation sends evidence. Source and image locators pass the input and path policy before capture; credential-looking locators are rejected. A URL source is fetched over HTTPS only, under the address policy at connection and every redirect, with bounded size and redirects, and no stored credential is sent to that origin. Image bytes stay out of tool responses, which return capture identity and a saved inspection path instead.

## Filesystem policy

- Allowed roots are the workspace root (`WORKSPACE_ROOT` / `local.workspaceRoot`, else the process cwd), `ALLOWED_PATHS` / `local.allowedPaths`, and `OCTOCODE_HOME`. The OS home is **not** allowed unless one of these covers it.
- Relative paths resolve against the workspace root.
- Relative traversal and paths outside the allowed roots fail with `outsideAllowedRoots` on every local tool.
- System directories such as `/etc` stay denied even when listed in `ALLOWED_PATHS`.
- Sensitive names are withheld by file name and by content, never by a folder's name: a `src/private/` or `secrets/` source folder is read, searched and listed like any other. Discovery prunes and every read denies credential-store directories (`.ssh`, `.aws`, `.kube`, `.gnupg`, …) and credential-named files: environment files (`.env*`), private keys and certificates (`*.pem`, `id_rsa*`), keystores, credential files, shell history, browser login stores, infrastructure state and wallets. A file such as `secrets/prod.env` or `private/key.pem` stays withheld by its own name; any other file's secret-shaped text is redacted by the content scanner.
- Symlinks are revalidated against their canonical targets; escaped descendants are rejected.
- File type, size, mutation, and snapshot checks run before evidence is returned.

Denial messages name the path as requested or relative to `~`; outside-root denials list the allowed roots.

`OCTOCODE_ENABLE_LOCAL=false` disables local tools. `clasify` requires a nonblank classification credential and a successful provider ping; it is independent of `OCTOCODE_BETA`.

## External processes

Search, AST, providers, response shaping, and bulk orchestration are embedded Rust. External processes run only for:

- system Git for `ghCloneRepo`: HTTPS-only remotes, bounded arguments and output, process-group cancellation, staged publication, no token in argv;
- configured language servers, launched without a shell and owned by the native LSP pool;
- `gh auth token`, the last GitHub credential source.

Language-server provisioning accepts only pinned assets from allowed HTTPS hosts, verifies SHA-256 before extraction, uses per-target locks, and publishes atomically.

## GitHub credentials

Resolution, login, refresh, and logout: [AUTHENTICATION.md](AUTHENTICATION.md). Security properties:

- Tool query fields never accept tokens. An env token goes only to the host of `GITHUB_API_URL`, which only the shell or home config can set.
- `credentials.json` uses AES-256-GCM (16-byte IV, authentication tag, ciphertext) with the key in `.key`. Both are `0600` on Unix; a newly created home directory is `0700`. Writes are locked and atomic. Corrupt or unauthenticated files are rejected and not overwritten; symlinked credential files and Unix hard links are rejected. The adjacent key does not protect against someone who can read both files.
- `gh auth token` runs without Octocode's token variables, bounded to 5 seconds.
- `.octocoderc` never supplies GitHub tokens, and every `.env` file blocks bootstrap variables such as `PATH`, `HOME`, and `NODE_OPTIONS` ([rules](CONFIGURATION.md#env--environment-fallback)).

GitHub endpoint, credential, session, and cache identities are partitioned. Pagination redirects must stay same-origin. Retries and rate-limit delays are bounded, response bodies are capped, and GraphQL partial failures stay explicit.

## Cancellation and lifecycle

Requests enter through bounded queues. Cancellation and timeouts propagate through provider requests, filesystem work, Git, and LSP. A dropped caller releases its admission and resources; shutdown joins workers and ends pooled processes. Partial or capped evidence is never labeled complete without an executable continuation or an explicit terminal diagnostic.

## Reporting vulnerabilities

Open a private GitHub security advisory for the Octocode repository, not a public issue.
