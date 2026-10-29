Load when a skill needs an Octocode setting or provider credential. Source: `skills/common`; installed copies are generated.

## Inspect without exposing secrets

Use the task’s selected CLI: prefer the built local Octocode CLI when available, otherwise an installed `octocode` or `npx -y octocode`. Examples use the portable `npx` prefix; substitute the selected CLI. From another workspace, invoke the local CLI by its absolute path so relative inputs keep their meaning:

```bash
npx -y octocode config home
npx -y octocode config check KEY
```

`config home` locates the actual Octocode home; do not hard-code a user's home. `config check` reports whether a key is set without printing its value (exit `0` set, `1` unset). Inspect only keys relevant to the task.

`config get KEY` prints the resolved value. For a credential, capture it directly into the consuming command's environment; never echo it, include it in a tool result, report, brief, URL, or shell trace.

```bash
PROVIDER_KEY="$(npx -y octocode config get PROVIDER_KEY)" node <skill-folder>/scripts/<consumer>.mjs
```

Replace `PROVIDER_KEY` with the key that the consuming skill documents. A missing key follows that consumer's contract: skip an optional provider or report the required dependency; do not silently choose a paid provider.

## Configuration ownership

Resolved key precedence is process environment, trusted workspace `.octocode/.env`, then Octocode home's `.env`. The config package owns lookup and parsing; scripts receive environment values and do not add another dotenv or home resolver.

List the needed key and its consumer in the skill's provider instructions. Keep provider-specific setup, pricing, consent, and failure behavior there.

Inspect configuration before changing it. Change only what the task authorizes; capability failure does not authorize enabling beta tools, replacing credentials, or changing storage. When the user authorizes saving a secret, use the CLI's `config set KEY --stdin` rather than putting the value in shell history.
