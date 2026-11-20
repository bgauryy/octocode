# Security

Octocode runs on your machine. The CLI and MCP server share one set of path, credential and content rules. Returned text is scanned before it becomes part of the calling agent's conversation. To report a vulnerability, use the repository's [private security advisory channel](https://github.com/bgauryy/octocode/security/advisories/new).

## What leaves your machine

| Action | Sent to |
|---|---|
| Local research, graph builds | Nowhere. Snapshots stay under your home or workspace |
| GitHub search, read, history, clone | GitHub or your Enterprise host, with that host's credential |
| Package lookup | The package registry and its source host |
| `securityCheck` | OSV, GitHub, public registries. [Advisory egress](#advisory-egress) |
| `clasify`, `classification` | The provider. [Classification egress](#classification-egress) |

## Files and credentials

Relative paths resolve against `local.workspaceRoot`; `local.allowedPaths` adds roots. Sensitive directories stay restricted.

`security.credentialFiles` defaults to `withhold`, which skips dotenv files (`.env`, `.env.*`, `.envrc`), keys and keystores (`id_rsa*`, `*.pem`, `*.key`, `*.p12`, `*.jks`), named stores (`.netrc`, `.npmrc`, `.pypirc`, `.pgpass`, `.git-credentials`, `credentials.json`, `kubeconfig`, `terraform.tfstate`, `terraform.tfvars`), their backups (`.bak`, `.old`, `~`), and whole directories (`.ssh`, `.aws`, `.azure`, `.kube`, `.docker`, `.gnupg`, `.password-store`). Other files, such as `~/.zshrc`, stay readable. Allow with `security.credentialFiles: allow` or `OCTOCODE_CREDENTIAL_FILES=allow` in a trusted setting. Graphs and classification always withhold them, and allowed reads keep saved local results in memory. [All settings](CONFIGURATION.md#all-settings-and-defaults)

## Secrets in results

Known secret formats become `[REDACTED-<PATTERN>]`: only the matched secret is replaced, and line numbers do not change. Formats include provider tokens (GitHub, GitLab, AWS, Google, Slack, Stripe, OpenAI, Anthropic, npm and others), private-key armor, JSON Web Tokens, connection strings with a password, known credential keys with a fixed shape (`aws_secret_access_key`), and crates.io and Terraform Cloud tokens.

A key named `password`, `passwd`, `pwd`, `secret`, `client_secret`, `secret_key`, `private_key`, `token`, `api_key` or `apikey` loses a literal value in source, JSON, YAML, `.ini` or `.properties`; a bare value is redacted only in config files. A URL query value is redacted only behind a secret key such as `access_token`; paging tokens (`page_token`, `next_token`) read as written. Any dotenv file has every value replaced. Placeholders (`${X}`, `changeme`, `<x>`), booleans, values under four characters and paths are not secrets. Redaction is pattern-based, so an unlisted format can still appear.

`output.redactEmails` (commit-author emails in GitHub results) and `output.redactUuid` (every UUID) are both `false`. Enable them with `config set` or `OCTOCODE_REDACT_EMAILS` and `OCTOCODE_REDACT_UUID`.

## Trust for project code

| Setting | Default | Effect |
|---|---|---|
| `lsp.trustProjectConfig` | `false` | A project's language-server config cannot launch an executable until trusted |
| `lsp.trustFetchedCode` | `false` | No language server on fetched checkouts, materialized files or URL captures |

Language servers can run project code, so use them only on projects you trust. Check routing with `octocode lsp-server status <file>`.

## Classification egress

Listing catalog tools sends nothing. A `clasify` call, or a tool with `classification`, sends questions, context and selected evidence to the provider, after access and redaction checks. The default provider is Jev at `https://api.typesafe.ai`; set `OCTOCODE_CLASSIFICATION_API_HOST` and `OCTOCODE_CLASSIFICATION_API` in home or process configuration, then restart MCP. URL captures fetch a public HTTPS page with no stored credential. PNG and JPEG pixels are not text-redacted: treat a sent image as disclosed. [Classification guide](OCTOCODE_CLASIFY.md)

## Advisory egress

`securityCheck` is on by default in the CLI and opt-in over MCP ([availability](OCTOCODE_TOOLS.md#securitycheck)).

| Operation | What is sent |
|---|---|
| `package` | OSV gets the name and any listed version; GitHub can get coordinates and repository. Other ecosystems send name and version to OSV directly |
| `repository` | GitHub gets `owner/name`; OSV gets advisory IDs only after GitHub identifies the repository as public |
| `commit` | OSV gets the full hash, even for private commits. With `repo`, GitHub gets both |
| `advisory` | OSV gets the ID; for a GHSA, GitHub also gets the ID and any repository given |

## GitHub credentials

Tool queries take identifiers, never tokens. A token comes from a supported environment variable, an Octocode sign-in or the GitHub CLI, in that order, and the configured API host decides where it is sent. [Token resolution](AUTHENTICATION.md#github-token-resolution)

`credentials.json` and its `.key` in the Octocode home hold your sign-in, AES-256-GCM encrypted (owner-only on Unix; both files together decrypt it). `.env` in the home or workspace `.octocode/` holds `config set` keys in plain text.

Remove the sign-in with `octocode auth logout`. `ghCloneRepo` runs system Git with host-scoped credentials through an askpass helper. Language servers download only after their pinned digest matches.
