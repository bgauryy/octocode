Load when a skill needs an Octocode setting or provider credential. Source: `skills/common`; installed copies are generated.

## Inspect without exposing secrets

Use the task's selected CLI: prefer the built local Octocode CLI when available, otherwise an installed `octocode` or `npx -y octocode`. Examples use the portable `npx` prefix; substitute the selected CLI. From another workspace, invoke the local CLI by its absolute path so relative inputs keep their meaning.

```bash
npx -y octocode config home          # the actual Octocode home; do not hard-code it
npx -y octocode config check KEY     # exit 0 set, 1 unset; never prints the value
npx -y octocode config get KEY       # the value that applies; secrets stay masked
npx -y octocode config list          # every setting: value, source, default
```

`KEY` is a variable or a setting path (`classification.apiHost`). Inspect only keys relevant to the task.

## Where a value can live

For each key separately, the first valid value wins:

| Order | Place | Path |
|---|---|---|
| 1 | Process environment, or an MCP client `env` block | none |
| 2 | Workspace `.env` | `<workspace>/.octocode/.env` |
| 3 | Home `.env` | `<home>/.env` |
| 4 | Workspace `.octocoderc` | `<workspace>/.octocode/.octocoderc` |
| 5 | Home `.octocoderc` | `<home>/.octocoderc` |
| 6 | The setting's default | [settings and defaults](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md#all-settings-and-defaults) |

A workspace file cannot set a home-only value, such as `classification.apiHost` or `github.apiUrl`. Provider keys for skills live in the `.env` files, not `.octocoderc`.

## Change a value

Change only what the task authorizes. A capability failure does not authorize enabling beta tools, replacing credentials or changing storage.

```bash
npx -y octocode config set KEY --stdin     # a secret: keeps it out of shell history
npx -y octocode config set KEY VALUE       # anything else
npx -y octocode config unset KEY           # back to the next source
```

`set` and `unset` write the home `.env`. Keep values out of tool output, reports and logs. For one command, set the documented environment variable on that command instead.

## Provider scripts

Scripts that use Octocode credentials call the bundled [shared config loader](../scripts/lib/config-env.mjs) before reading `process.env`. It applies the home and trusted workspace `.env` files using `@octocodeai/config`, and process environment values keep precedence. The installed skill carries the loader, so it needs no `node_modules` or repository checkout at runtime. Scripts receive environment values and add no other dotenv or home resolver.

List the needed key and its consumer in the skill's provider instructions. Keep provider-specific setup, pricing, consent and failure behavior there.
