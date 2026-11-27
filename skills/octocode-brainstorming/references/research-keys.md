Load when checking or saving search keys, or when `search.mjs` reports an error or a provider status other than success.

# Search keys and search errors

## Keys

[search.mjs](../scripts/search.mjs) and [verify-claims.mjs](../scripts/verify-claims.mjs) read keys only through `getKey` in [keys.mjs](../scripts/keys.mjs). The first place that sets a key wins, in the [API key order](configuration.md#where-a-value-can-live): process environment, then the home `.env` (`OCTOCODE_HOME`, default `~/.octocode`), then the workspace `.octocode/.env` (the current directory).

```bash
node <skill-folder>/scripts/keys.mjs get TAVILY_API_KEY EXA_API_KEY SERPER_API_KEY DEEPSEEK_API_KEY   # set/missing, source; exit 1 if one is missing
node <skill-folder>/scripts/keys.mjs list                                                             # every declared Octocode key
node <skill-folder>/scripts/keys.mjs set TAVILY_API_KEY --stdin [--workspace]                         # paste or pipe; saves to the home .env
```

- No command prints a value; `files` names the home and workspace files read. A missing key carries its `fix`.
- Run from the workspace root. From a subfolder, `<workspace>` is that subfolder, so a key only in the repository's `.octocode/.env` is missed; the `workspace` path under `files` in the output shows which file was read.
- A workspace `.env` only fills a key you have not set globally. `TAVILY_BASE_URL`, `EXA_BASE_URL`, `SERPER_BASE_URL` and `DEEPSEEK_BASE_URL` come only from the process environment or the home `.env`, so a repository cannot send your key to its own host.
- Keep keys out of briefs, ledgers, reports and logs. Other settings: [shared configuration](configuration.md).

| Search output | Meaning | Action |
|---|---|---|
| `error.code: "missingKey"`, exit 1 | No key for the selected providers; the message names the files read | Save a key with `keys.mjs set`, or rerun from the folder whose `.octocode/.env` holds it; meanwhile use the host's web search and fetch |
| provider `status: "skipped"` | That key is not set, or the vendor has no search of that `--kind` (`reason` says which); the others ran | None, unless that vendor's surface is needed |
| provider `status: "keyRejected"` | HTTP 401 or 403 on every query: the key is wrong or revoked | Tell the user to renew it with the row's `fix` |
| provider `status: "quotaExceeded"` | Exa or DeepSeek 402, Tavily 432 or 433: out of credits or over a plan limit | Tell the user to add credits at the row's `fix` link; use the other vendors meanwhile |
| provider `status: "rateLimited"` | HTTP 429 on every query; `retryAfterSeconds` when the vendor sent one | Wait that long, then retry with fewer queries |
| `error.code: "keysRejected"` / `"providersUnavailable"` | Every configured vendor rejected its key / failed for one of the reasons above | Apply each provider's fix; gather with host tools meanwhile |
| provider `status: "error"` with `errors[]` | Timeout or server error | Retry once with fewer queries; do not repeat an unchanged failing call |
| `error.code: "invalidArguments"`, exit 2 | A bad flag, or `--providers` names only vendors without that `--kind` (`--providers deepseek --kind news`) | Fix the flag; the message lists the vendors that serve the kind |
| `error.code: "noResults"` | Every provider answered with nothing | Reword, change `--kind` or drop filters |
