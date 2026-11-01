Load when a skill needs an Octocode setting or provider credential. Source: `skills/common`; installed copies are generated.

## Inspect without exposing secrets

Use the task’s selected CLI: prefer the built local Octocode CLI when available, otherwise an installed `octocode` or `npx -y octocode`. Examples use the portable `npx` prefix; substitute the selected CLI. From another workspace, invoke the local CLI by its absolute path so relative inputs keep their meaning:

```bash
npx -y octocode config home
npx -y octocode config check KEY
```

`config home` locates the actual Octocode home; do not hard-code a user's home. `config check` reports whether a key is set without printing its value (exit `0` set, `1` unset). Inspect only keys relevant to the task.

Provider scripts that use Octocode credentials call the bundled [shared config loader](../scripts/lib/config-env.mjs) before reading `process.env`. It applies the home and trusted workspace `.env` files using `@octocodeai/config`; process environment values keep precedence. The installed skill carries the loader, so it does not need `node_modules` or a repository checkout at runtime.

Use `config check KEY` to check whether a value is available without printing it. When you want to save a credential, use `npx -y octocode config set KEY --stdin`; keep the value out of shell history, tool output, reports and logs. For a one-command override, set the documented environment variable directly for the command.

## Configuration ownership

Resolved key precedence is process environment, trusted workspace `.octocode/.env`, then Octocode home's `.env`. The config package owns lookup, parsing and propagation; scripts receive environment values and do not add another dotenv or home resolver.

List the needed key and its consumer in the skill's provider instructions. Keep provider-specific setup, pricing, consent, and failure behavior there.

Inspect configuration before changing it. Change only what the task authorizes; capability failure does not authorize enabling beta tools, replacing credentials, or changing storage. When the user authorizes saving a secret, use the CLI's `config set KEY --stdin` rather than putting the value in shell history.
