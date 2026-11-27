Load when a skill needs an Octocode setting or provider credential.
<!-- Source: skills/common; installed copies are generated. -->

## Inspect without exposing secrets

Use the task's selected CLI: prefer the built local Octocode CLI when available, otherwise an installed `octocode` (`octocode-cli` in Octocode Code agent shells, where `octocode` is the coding agent) or `npx -y octocode`. Examples use this checkout's CLI; substitute the selected CLI. From another workspace, invoke the local CLI by its absolute path so relative inputs keep their meaning.

```bash
node packages/octocode/out/octocode.js config home          # the actual Octocode home; do not hard-code it
node packages/octocode/out/octocode.js config check KEY     # exit 0 set, 1 unset; never prints the value
node packages/octocode/out/octocode.js config get KEY       # the value that applies; secrets stay masked
node packages/octocode/out/octocode.js config list          # every setting: value, source, default
```

`KEY` is a variable or a setting path (`classification.apiHost`). Inspect only keys relevant to the task.

## Where a value can live

For each key separately, the first valid value in its column's order wins:

| Place | Path | Setting order | API key order |
|---|---|---|---|
| Process environment, or an MCP client `env` block | none | 1 | 1 |
| Workspace `.env` | `<workspace>/.octocode/.env`; the workspace is the current directory | 2 | 3 |
| Home `.env` (global) | `<home>/.env`; `<home>` is `OCTOCODE_HOME`, default `~/.octocode` | 3 | 2 |
| Workspace `.octocoderc` | `<workspace>/.octocode/.octocoderc` | 4 | none |
| Home `.octocoderc` | `<home>/.octocoderc` | 5 | none |
| The setting's default | [settings and defaults](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md#all-settings-reference) | 6 | none |

API keys are the credentials skill scripts and the CLI read (GitHub tokens, Tavily, Exa, Serper, DeepSeek, ScrapingAnt, classification). A workspace file cannot set a home-only value, such as `classification.apiHost` or `github.apiUrl`. Provider keys live in the `.env` files, not `.octocoderc`.

## Change a value

Change only what the task authorizes. A capability failure does not authorize enabling beta tools, replacing credentials or changing storage.

```bash
node packages/octocode/out/octocode.js config set KEY --stdin     # a secret: keeps it out of shell history
node packages/octocode/out/octocode.js config set KEY VALUE       # anything else
node packages/octocode/out/octocode.js config unset KEY           # back to the next source
```

`set` and `unset` write the home `.env`. Keep values out of tool output, reports and logs. For one command, set the documented environment variable on that command instead.

## API keys in skill scripts

A skill whose scripts call a provider ships a `keys.mjs` script beside them: the `getKey` lookup from `@octocodeai/config`, built into the skill so it runs with no packages. Its scripts read keys only through `getKey`, in the API key order of the [table above](#where-a-value-can-live), and its provider instructions give the `keys.mjs` commands (`list`, `get`, `set --stdin`).

A workspace file only fills a key you have not set globally, and never sets a home-only key or an endpoint, so a repository cannot swap in its own key or redirect yours. Run the skill's scripts from the workspace root; from a subfolder, that subfolder is the workspace. `keys.mjs` never prints a value, and `set` reads it from stdin. `octocode config get KEY` reports the same source for an API key as `keys.mjs`.

List the needed key and its consumer in the skill's provider instructions. Keep provider-specific setup, pricing, consent and failure behavior there.
