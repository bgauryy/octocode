# Security

Octocode is built for agent workflows where the context window can fill with secrets, tokens, and untrusted paths. Its core security principle:

> **Every byte that reaches the model is scanned and redacted first.** Secrets are stripped on the way *in* (tool inputs) and on the way *out* (tool results) — they never reach the LLM or error messages.

Security enforcement is centralized in `@octocodeai/octocode-engine`: native Rust scanning/minification plus the engine's TypeScript validation wrappers, registries, and path/command guards. The same package is used by the MCP server and the CLI.

---

## 1. The sanitization pipeline

Every tool call — over MCP or CLI — follows the same security-first path:

```
client args
  → validate + sanitize INPUTS        (withSecurityValidation → ContentSanitizer.validateInputParameters)
  → run tool (GitHub API / local FS / LSP)
  → sanitize FETCHED CONTENT on read  (ContentSanitizer.sanitizeContent per file / response)
  → mask OUTPUT at the boundary       (sanitizeCallToolResult / ContentBuilder → ContentSanitizer)
  → compact YAML/JSON result + hints[]
```

Redaction happens at **three** points, not one:

| Stage | Where | What it protects against |
|-------|-------|--------------------------|
| **Input** | `withSecurityValidation` wraps every tool handler | A secret pasted into a query argument being echoed back |
| **Content** | Each reader (`localGetFileContent`, ripgrep, structural search, binary inspect, find, view-structure, GitHub code/file fetch, npm) sanitizes content as it is read | A `.env`, key file, or repo file with embedded credentials being surfaced verbatim |
| **Output** | `callToolResult` scans and masks every returned text item | Any secret that slipped through earlier stages reaching the model |

When content contains secrets, they are masked **and** the result carries a warning, e.g. `Secrets detected and redacted: <types>`, so the agent knows redaction occurred rather than silently seeing altered text.

---

## 2. Secret detection

The scanner (`RegexSet`-based, compiled in Rust, with a TypeScript fallback from the same canonical pattern list) covers **300+ provider-specific credential patterns** plus generic high-risk formats:

- **Cloud:** AWS (access key ID, secret access key, session token, ARNs, account IDs), Azure (AD client secret, storage/Cosmos/Service Bus connection strings, subscription IDs, OpenAI), GCP / Google API keys, Alibaba Cloud.
- **AI providers:** OpenAI, Anthropic, Azure OpenAI, Amazon Bedrock, AI21, AssemblyAI, and more.
- **SaaS / dev tools:** GitHub (PAT, fine-grained, OAuth, app), Slack, Stripe, npm, Atlassian, Asana, Airtable, Algolia, Auth0, Adyen, 1Password, Artifactory, and many others.
- **Generic / structural:** JWTs, PEM / private keys, SSH keys, bearer tokens, passwords in URLs, database connection strings (Postgres, MySQL, MongoDB, Redis, CouchDB, Elasticsearch), `.NET` connection strings, and high-entropy strings.

Detected values are replaced with a masked placeholder; the original is never returned.

---

## 3. Path validation (local tools)

Local filesystem access is bounded by a multi-layer validator before any read:

1. Resolve relative input from `WORKSPACE_ROOT` / configured workspace root / `cwd`, then normalize it (resolve `.`/`..`, expand `~`).
2. Prefix-check against the engine's allowed roots: the user's home directory by default, `ALLOWED_PATHS` entries, and roots registered by Octocode itself (using `path + separator`, so a sibling like `/repo-evil` cannot bypass `/repo`).
3. Apply the ignore filter (sensitive files/dirs — keys, `.env`, credential stores — are blocked by default).
4. Resolve symlinks with `realpath`, then **re-validate** the real target (symlink escapes are caught).

`ALLOWED_PATHS` adds roots; an empty list is **not** unrestricted. The default validator includes `HOME`, while stricter embedded/test validators can opt out with `includeHomeDir:false`. Local tools are enabled by default, can be disabled with `ENABLE_LOCAL=false`, and always use the same validator.

### Blocked sensitive files and directories

The ignore filter rejects known secret-bearing paths wherever they appear (a blocked path returns a redacted error, never contents). It matches on both file-name patterns and directory patterns:

- **Keys and certs:** `*.pem`, `*.key`, `*.crt`, `*.cer`, `*.csr`, `*.p12`, `*.pfx`, `*.jks`, `*.keystore`, `*.ppk`; SSH material (`id_rsa`/`id_dsa`/`id_ecdsa`/`id_ed25519` and `.pub` variants, `*_rsa`, `*_ed25519`, `authorized_keys`, `known_hosts`, `.ssh/`, `.ssh/config`).
- **Credentials and tokens:** `.env` and `.env.*`, `.netrc`/`_netrc`, `.npmrc`, `.pypirc`, `.dockercfg`, `.pgpass`, `.my.cnf`, `.git-credentials`, `.htpasswd`, `*_token` / `.token` / `access_token` / `refresh_token` / `bearer_token`, `client_secret*.json`, `oauth2*.json`, `auth.json`, `*service-account*.json`, `application_default_credentials.json`, `.npm-token`, `.slack-token`, `.github-token`, vault pass files.
- **Cloud and infra:** `.aws/` (`credentials`, `config`), `.azure/`, `.config/gcloud/`, `.kube/` / `kubeconfig`, `.docker/`, `.terraform/`, `*.tfstate`, `*.tfvars`, `.s3cfg`.
- **OS and application secret stores:** `.git/`, `secrets/`, `private/`, `.password-store/`; browser login data and key DBs (Chrome/Chromium, Firefox), OS keychains (`Library/Keychains/`, `*.keychain`), password managers (`*.kdbx`, `*.kdb`, 1Password vaults); shell history (`.bash_history`, `.zsh_history`, `.*_history`, DB shell histories); crypto wallets (`wallet.dat`, `.bitcoin/`, `.ethereum/`, `.electrum/`); OS account stores (`shadow`, `SAM`, `NTUSER.DAT`); core dumps and DB dumps.
- **Misc app secrets:** `wp-config.php`, `google-services.json`, `GoogleService-Info.plist`, `*.mobileprovision`, `.idea/dataSources.xml`, Maven `settings.xml`, `gradle.properties`, `*.ovpn`, `wireguard.conf`, `secrets.yml`, `master.key`, and `PASSWORDS.md`/`SECRETS.md`/`CREDENTIALS.md`.

The canonical lists are `IGNORED_FILE_PATTERNS` and `IGNORED_PATH_PATTERNS` in the engine (`src/security/filePatterns.ts`, `src/security/pathPatterns.ts`).

---

## 4. Command execution

- Normal local text search runs ripgrep in-process inside `octocode-engine`; structural search and file enumeration also run through engine/file APIs rather than user-provided shell strings.
- Where Octocode invokes external helpers, command names and arguments are allowlisted. The base command allowlist is **`rg`, `grep`, `find`, `ls`, and restricted `git`** (`clone` / `sparse-checkout` only); binary/archive modes register a fixed set of format and archive helpers such as `file`, `tar`, `unzip`, `7z`, and decompression readers.
- External commands run via `child_process.spawn()` with an argument array — **never** `exec` with a shell string — and dangerous shell metacharacters are rejected before execution.
- `include` / `exclude` / `excludeDir` are glob patterns, not paths, and cannot escape the validated search root.

---

## 5. Credentials & tokens

- GitHub auth resolves in priority order: `OCTOCODE_TOKEN` → `GH_TOKEN` → `GITHUB_TOKEN`, then encrypted on-disk Octocode OAuth credentials, then the `gh` CLI token.
- On-disk OAuth credentials are stored **AES-256-GCM encrypted** under `OCTOCODE_HOME`.
- Tokens are read from the environment / secure store at request time and are themselves subject to output masking — they are never echoed in results.
- See [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md) and [Credentials Architecture](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md#credential-architecture-api).

---

## 6. Input limits

`ContentSanitizer` also bounds untrusted input shape: strings are capped (~10K chars), arrays (~100 items), and object nesting (~20 levels). Agent-facing numeric ranges (depth, context lines, limits, pagination offsets) are clamped at the schema layer.

---

## 7. Scope & disclosure

**Octocode protects the agent context boundary** — what flows between untrusted code/content and the model. It does **not** replace repository secret-scanning, OS-level sandboxing, or network egress controls; run those alongside it.

To report a vulnerability, open a private advisory on the [repository](https://github.com/bgauryy/octocode) rather than a public issue.
