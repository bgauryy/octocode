# Configuration

Load when setting up credentials or diagnosing configuration. Why: the launcher follows Octocode home/env conventions; the native binary reads only its process environment.

Get a key from [TypeSafe's key console](https://console.typesafe.ai/keys). The simplest setup is a line `OCTOCODE_JEV_KEY=your-key` in `<HOME>/.octocode/.env`. The launcher reads it on every invocation without shell exports or reloading. A secret manager or process environment works too. The launcher forwards the key as a Bearer credential. It does not use `TYPESAFE_API_KEY` implicitly, accept secret command-line flags, or print the key.

With `node scripts/jev.mjs`, the effective Octocode home is `OCTOCODE_HOME` when set, otherwise `<home>/.octocode`. Supported sources, highest precedence first:

| Source | Location/shape |
| --- | --- |
| Process environment | `OCTOCODE_JEV_KEY` |
| Trusted project env, opt-in | `<cwd>/.octocode/.env`, only with `--project-env` |
| Global env | `<octocode-home>/.env` |
| Octocode config | `<octocode-home>/.octocoderc`: `env.OCTOCODE_JEV_KEY`, or top-level `OCTOCODE_JEV_KEY` |

Empty/whitespace values fall through. When both config forms contain a value, the `env` value takes precedence. Project `.octocoderc` files and arbitrary `.env` files are not auto-discovered. `--project-env` means the caller trusts that workspace's env file. Only the three Jev variables and the shared `REQUEST_TIMEOUT` / `MAX_RETRIES` variables are imported from configuration files into the child process.

Merge this fragment into your existing `.octocoderc`; do not overwrite unrelated fields:

```json
{
  "env": {
    "OCTOCODE_JEV_KEY": "replace-with-your-key",
    "OCTOCODE_JEV_MODEL": "jev-latest"
  }
}
```

Or place `OCTOCODE_JEV_KEY=replace-with-your-key` in the global `.env`. The shared parser handles simple `KEY=value`, optional `export`, and enclosing quotes. It does not perform shell expansion or multiline dotenv parsing; keep inline comments off value lines. Prefer plain JSON in `.octocoderc`. Malformed config fails before a request, without echoing its contents. Restrict secret-file access to your account and keep it out of version control.

## Options

| Setting | Resolution |
| --- | --- |
| API key | `OCTOCODE_JEV_KEY` using the source precedence above |
| Model | `--model` > request `model` > `OCTOCODE_JEV_MODEL` > `jev-latest` |
| API root | `--base-url` > `OCTOCODE_JEV_BASE_URL` > `https://api.typesafe.ai` |
| Total HTTP/retry timeout | `--timeout-ms` > `REQUEST_TIMEOUT` > `.octocoderc` `network.timeout` > 30000 ms |
| Additional retries | `--retries` > `MAX_RETRIES` > `.octocoderc` `network.maxRetries` > 3 |

The launcher uses the shared Octocode `resolveNetwork` implementation. Network env variables use the same process/project/global/config-env precedence as Jev variables. Shared timeouts are clamped to 5000–300000 ms and retry counts to 0–10; explicit CLI timeout accepts 100–300000 ms. For example, `.octocoderc` may contain `"network": { "timeout": 45000, "maxRetries": 1 }`. Timeout means a total HTTP/retry budget here. The raw Rust binary retains its own defaults (30000 ms, two retries); use the launcher for shared config resolution.

Existing GitHub, local search, tools, LSP, storage and output sections can coexist in the same `.octocoderc`. They configure their owning Octocode tools, not this Jev HTTP client. Jev always emits its complete JSON API result; Octocode YAML/pagination preferences do not truncate or reformat it.

The launcher applies the same env/config lookup to `OCTOCODE_JEV_MODEL` and `OCTOCODE_JEV_BASE_URL`. A custom API root receives the credential, so configure only a trusted endpoint. Roots must have no path, query, fragment or user information; the client appends `/v1/systemone` or `/v1/models`. HTTPS certificate verification stays enabled. Plain HTTP is limited to loopback for offline tests; redirects never forward credentials. Standard process proxy configuration follows the HTTP library.

From any workspace, invoke `node <skill-dir>/scripts/jev.mjs ...` with the absolute skill path; this preserves the caller's workspace for `--project-env`. For raw binary use, export the key and other desired Jev variables first. The binary does not read `.octocoderc` or `.env` itself.

After setup, use `references/protocol.md` to prepare a request and interpret its result.
