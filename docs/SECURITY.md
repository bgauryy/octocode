# Security

Octocode runs on your machine. The CLI and MCP server share one set of path, credential and content rules. Returned text is scanned before it becomes part of the calling agent's conversation. To report a vulnerability, use the repository's [private security advisory channel](https://github.com/bgauryy/octocode/security/advisories/new).

## What leaves your machine

| Action | Sent to |
|---|---|
| Local file search, reads and graph builds | Files are processed locally. Graph snapshots go to the configured directory. |
| GitHub search, read, history, clone | GitHub or your Enterprise host, with that host's credential |
| Package lookup | The package registry and its source host |
| `securityCheck` | OSV, GitHub, public registries. [Advisory egress](#advisory-egress) |
| `clasify` | The provider. [Classification egress](#classification-egress) |

### Preview an outbound query

Set `preview: true` on a query row for `artifactSearch`, `securityCheck`, or a fresh `clasify` request. Its `data.preview` describes the recipients and selected data locally. Remote source contents are known only after a live fetch. Each row in a batch is independent. Set the flag on every row you want to preview. Run a fresh query without the flag to contact the providers.

## Files and credentials

Relative paths resolve against `local.workspaceRoot`; `local.allowedPaths` adds roots. Sensitive directories stay restricted.

Octocode withholds credential files such as `.env`, private keys and npm credentials by default. You can change `security.credentialFiles` for trusted local reads. Graph and classification tools always withhold these files. [Configuration](CONFIGURATION.md#all-settings-and-defaults)

## Secrets in results

Recognized secrets are replaced with a redaction marker in text results. Line numbers stay the same. Pattern matching can miss unfamiliar formats. Enable `output.redactEmails` or `output.redactUuid` if you also want those values masked.

## Trust for project code

| Setting | Default | Effect |
|---|---|---|
| `lsp.trustProjectConfig` | `false` | A project's language-server config cannot launch an executable until trusted |
| `lsp.trustFetchedCode` | `false` | No language server on fetched checkouts, materialized files or URL captures |

Language servers can run project code, so use them only on projects you trust. Check routing with `octocode lsp-server status <file>`.

## Classification egress

Listing catalog tools sends nothing. A `clasify` call sends questions, context (a `clasify` row's `mainGoal` and `background`, never its `reasoning`) and selected evidence to the provider, after access and redaction checks. The default provider is Jev at `https://api.typesafe.ai`; set `OCTOCODE_TYPESAFE_HOST` and `OCTOCODE_TYPESAFE_KEY` in home or process configuration, then restart MCP. URL captures fetch a public HTTPS page with no stored credential. PNG and JPEG pixels are not text-redacted: treat a sent image as disclosed. [Classification guide](OCTOCODE_CLASIFY.md)

## Advisory egress

`securityCheck` is on by default in the CLI and opt-in over MCP ([availability](OCTOCODE_TOOLS.md#securitycheck)).

| Operation | What is sent |
|---|---|
| `package` | OSV receives the name and version when supplied, including names missing from a registry. Registry and GitHub lookups can also receive package coordinates. |
| `url` | Octocode extracts coordinates from a supported registry page URL or package URL (purl), then checks them as a package. |
| `repository` | GitHub gets `owner/name`; OSV gets advisory IDs only after GitHub identifies the repository as public |
| `commit` | OSV receives the full hash. Use a public commit. When you supply a repository, GitHub also receives the repository and hash. |
| `advisory` | OSV gets the ID; for a GHSA, GitHub also gets the ID and any repository given |

## GitHub credentials

Tool queries take identifiers, never tokens. The configured GitHub host determines which credential Octocode selects and where it sends it. [Token resolution](AUTHENTICATION.md#github-token-resolution)

Octocode stores your GitHub sign-in encrypted in its home. Protect `credentials.json` and its companion key together. `octocode auth logout` removes the saved sign-in. Credentials supplied with `config set` are written to the home `.env` in plain text.
