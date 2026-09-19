# Configuration

Load when setting up credentials or diagnosing a failed call. Why: Jev provider settings are protected process-environment values; shared network settings also use Octocode configuration.

## Credentials

Get a key from [TypeSafe's console](https://console.typesafe.ai/keys). Supply `OCTOCODE_JEV_KEY` in the process environment or MCP client's environment. The launcher does not automatically import Jev credentials from `.octocoderc` or `.env`.

If your key is in `<home>/.octocode/.env`, explicitly load that trusted file with Node 20.6+:

```sh
node --env-file="$HOME/.octocode/.env" <skill-dir>/scripts/ask-file.mjs --files "src/retry.ts" --questions "Does this implement retry?"
node --env-file="$HOME/.octocode/.env" <skill-dir>/scripts/run-loop.mjs --input compact.json
```

Use the actual configured Octocode home when it differs. For a user-authorized project file, use `--env-file=<workspace>/.octocode/.env`. Run from the workspace so relative source paths resolve there. Never print the key, put it in packets, or commit the file. Node loads the file before the launcher; `--project-env` alone does not import protected Jev settings.

## Resolution

| Setting | Precedence |
|---|---|
| API key | Process `OCTOCODE_JEV_KEY` |
| Model | `--model` > request `model` > process `OCTOCODE_JEV_MODEL` > `jev-latest` |
| API root | `--base-url` > process `OCTOCODE_JEV_BASE_URL` > `https://api.typesafe.ai` |
| Total HTTP/retry timeout | `--timeout-ms` > `REQUEST_TIMEOUT` > config `network.timeout` > 30000 ms |
| Additional retries | `--retries` > `MAX_RETRIES` > config `network.maxRetries` > 3 |

Model and API-root flags belong to `scripts/jev.mjs`; higher-level runners expose their own options and may set request `model`. Pin it in a profile or reasoning packet when comparing runs, and record the resolved response model.

The launcher uses injected `scripts/octocode-config.mjs` for home and network settings. Network env values resolve from process, trusted project `.env` with `--project-env`, global `.env`, then config env. Shared timeout bounds are 5000–300000 ms and retries 0–10; explicit CLI timeout accepts 100–300000 ms. The raw native binary uses process settings only and defaults to two retries.

A custom API root receives the credential. Use a trusted origin with no path, query, fragment, or user information. HTTPS verification stays enabled; plain HTTP is limited to loopback tests, and redirects do not forward credentials.

For a missing-key error, check whether the process loaded the intended env file. For malformed config, fix it without exposing its contents. For request and transport errors, use `references/protocol.md`.
