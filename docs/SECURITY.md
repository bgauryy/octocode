# Security

The CLI and MCP server use the same path, credential and content rules. When you use an agent, the results it requests become part of that client's conversation. Which other service receives data depends on the tool you call.

| Action | Destination | Data sent |
|---|---|---|
| Local text, syntax or graph research | Local runtime | Reads files in the allowed scope. Graph builds also write snapshots. |
| GitHub search, read, history or clone | GitHub or your configured Enterprise host | Repository and query details, with the credential selected for that host when available. |
| Package lookup | The applicable registry and, for source verification, the source host | Package name, requested version or search terms. Private npm registries use matching npm credentials. |
| Known-advisory lookup | OSV, GitHub and applicable public registries | Identifiers depend on the operation. [Advisory egress](#advisory-egress) |
| Classification | The configured classification provider | Questions, context and selected evidence. [Classification egress](#classification-egress) |

## Filesystem policy

Relative paths resolve against the workspace root, which defaults to the process working directory. Set `local.workspaceRoot` to change it or `local.allowedPaths` to admit another directory. Octocode also admits its managed outputs, such as clone worktrees. It checks canonical paths and restricts sensitive locations even inside an allowed root. An outside-root error shows the allowed roots for that process.

| Default | Effect |
|---|---|
| `security.credentialFiles: "withhold"` | Discovery and reads withhold the known credential files and their backups (`.bak`, `.old`, `~`): the dotenv family (`.env`, `.env.*`, `.envrc`, `*.env`), private keys and keystores (`id_rsa*`, `*.pem`, `*.key`, `*.p12`, `*.jks`), and named credential stores (`.netrc`, `.npmrc`, `.pgpass`, `.git-credentials`, `credentials.json`, `kubeconfig`, Terraform state and variables, Octocode, Claude and Codex logins). Credential directories (`.ssh`, `.aws`, `.azure`, `.kube`, `.docker`, `.gnupg`, `.password-store`, `.config/gh`, `.config/gcloud`) are withheld whole. Every other file, such as `~/.zshrc` or a shell history, is readable. |
| `output.redactEmails: false` | Email addresses, such as commit authors, can appear in GitHub tool results. Enable the setting to mask them in those results; local source email addresses stay readable. |
| `output.redactUuid: false` | UUIDs can appear in results. Enable the setting to mask every UUID in a tool result. |
| `lsp.trustProjectConfig: false` | A project's custom server configuration requires trust before it can launch an executable. |
| `lsp.trustFetchedCode: false` | Language servers cannot start against checkouts, materializations or URL captures acquired by Octocode. Text and syntax tools can inspect admitted files. |
| `lsp.allowBuildScripts: false` | Rust queries asking to enable Cargo build scripts or procedural macros receive a policy error. |

A trusted home or process setting can set `security.credentialFiles` to `allow` for local reads and searches. Sensitive directories, Octocode's credentials and protected locations in your OS home stay restricted. Graphs and classification continue to withhold credential-named files. Allowing these reads keeps saved local results in memory. [All settings and workspace permissions](CONFIGURATION.md#all-settings-and-defaults)

## Content sanitization

The runtime redacts known secret formats from text evidence, including file reads, search results and provider output. One list of known patterns decides: provider token formats (GitHub, GitLab, AWS, Google, Slack, Stripe, OpenAI, Anthropic, npm and others), private-key armors, JSON Web Tokens, connection strings that carry a password, and well-known credential keys with a fixed value shape (`aws_secret_access_key = <40 characters>`). A key labelled `password`, `passwd`, `pwd` (quoted), `secret`, `client_secret`, `secret_key`, `private_key`, `token`, `api_key` or `apikey` that is assigned a literal, in source, JSON, YAML, `.ini` or `.properties`, has only its value replaced (`passwordAssignment`, `secretAssignment`, `tokenAssignment`); a template or placeholder (`${X}`, `$X`, `{{x}}`, `<x>`, `%s`, `changeme`), a boolean, a short number, a path and a word of letters alone (`str`, `getPassword`) are no value. Only the matched secret is replaced, by a `[REDACTED-<PATTERN>]` placeholder; the text around it stays as written, and a multi-line key keeps its line count. A dotenv file (`.env`, `.env.*`, `*.env`) is a known credential store, so wherever its text is shown, in a read under `allow`, a GitHub read or a commit or pull-request patch, every `KEY=value` value is replaced and each name stays. No value is judged by its length or look: a secret in no known format and under no such label can appear in results, and ordinary code that names a credential stays readable. `output.redactEmails` masks email addresses in GitHub tool results; `output.redactUuid` masks UUIDs in all tool results. Both settings default to off. Returned markers and warnings identify affected evidence. Treat an explicit image as disclosed when you send it for classification.

### Classification egress

Catalog discovery makes no classification request. A call to `clasify` or a tool with `classification` sends the questions or predicates, optional context and the selected evidence to the configured provider. File and tool sources pass their access and text-redaction checks first. Inline classification judges the requested results. A `clasify` URL capture fetches a public HTTPS resource without a stored credential for that origin.

Explicit PNG/JPEG assets can be resized or padded before transmission. Their pixels and metadata have no text-redaction guarantee. The default provider is Jev at `https://api.typesafe.ai`. The process or home configuration controls the host and key. Restart an MCP server after adding or removing the key. [Classification guide](OCTOCODE_CLASIFY.md)

### Advisory egress

`securityCheck` is available by default on the CLI and opt-in on MCP. OSV can receive package names and versions, advisory IDs or commit hashes. A commit lookup can disclose a private commit's hash. GitHub can receive the repository and the identifiers needed for its advisory lookup. Retain each record's source attribution when sharing results. [Data sent by each advisory operation](OCTOCODE_TOOLS.md#securitycheck)

## External processes

`ghCloneRepo` runs system Git to acquire a checkout under the Octocode home. Git receives host-scoped credentials through its askpass helper. Worktrees and their backing stores share a lifecycle. [Storage](STORAGE_AND_LIMITS.md)

A language server or IDE provider can load project configuration and execute project code. Use language services on projects you trust. Inspect a file's effective route with `octocode lsp-server status <file>`. Managed downloads are checked against pinned digests before installation. [Language-server setup and trust](../packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md)

## GitHub credentials

Tool queries take repository identifiers, never access tokens. Set a token in a supported environment source, sign in with Octocode, or use GitHub CLI. The configured API host controls where Octocode sends the GitHub credential. [Credential selection for each request](AUTHENTICATION.md#github-token-resolution)

`auth login` encrypts `credentials.json` with AES-256-GCM. The decryption key lives beside it in `.key`. Both files use owner-only permissions on Unix. Someone with access to both can decrypt the login. Keys saved with `config set` go into a local plaintext `.env` file. [Where configuration is stored](CONFIGURATION.md#set-a-value)

## Input validation

The runtime validates tool fields and operation combinations before execution, then applies access policy and scans returned text. A cursor is tied to its tool and access context. An invalid or expired cursor returns an error. Mixed batches report per-row failures. [Tool data contract](TOOL_DATA_CONTRACT.md)

## Reporting vulnerabilities

Report a suspected vulnerability through the repository's private security advisory channel. [Report a vulnerability](https://github.com/bgauryy/octocode/security/advisories/new)
