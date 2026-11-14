# Configure Octocode: folders, precedence and keys

Load when a setting, folder or key must be found, added or changed.

## Configuration and folders

| Location | Contents |
|---|---|
| `~/.octocode/` by default | Shared settings, credentials, installed skills and caches. `octocode config home` shows the actual path. `OCTOCODE_HOME` relocates it. |
| `<workspace>/.octocode/` | Project settings, skill outputs and graph data. Configuration uses the process working directory without searching parents. |

Each value resolves in order from process/MCP environment, workspace `.env`, home `.env`, workspace `.octocoderc`, home `.octocoderc`, then its default. Keys belong in `.env`; settings belong in `.octocoderc`, JSON that accepts comments. A workspace file cannot set a home-only value. [Configuration](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) · [All settings and defaults](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md#all-settings-and-defaults)

## Add and configure keys

Local tools need no keys. Add only what the user's task needs:

| To use | Set | Default |
|---|---|---|
| GitHub tools | `octocode auth login`, or `GH_TOKEN` / `GITHUB_TOKEN` | none |
| `clasify` and inline `classification` | `OCTOCODE_CLASSIFICATION_API`, a key from the [provider](https://docs.typesafe.ai/introduction) | unset (off) |
| A proxy or other provider for classification | `OCTOCODE_CLASSIFICATION_API_HOST`; HTTPS API root, home-only | `https://api.typesafe.ai` (Jev) |
| Web search in `octocode-brainstorming` | `TAVILY_API_KEY`, `EXA_API_KEY` or `SERPER_API_KEY` | unset |
| `octocode-scraping` | `SCRAPING_ANT` | unset |

Save only within the user's requested setup scope; resolve missing authority before changing credentials or destinations. Have the user enter secrets directly:

```bash
octocode config set OCTOCODE_CLASSIFICATION_API --stdin    # the key, read from stdin into the home .env
octocode config set classification.apiHost https://gateway.example.com    # optional: another host
octocode config check OCTOCODE_CLASSIFICATION_API          # exit 0 set, 1 unset; prints no value
octocode config get classification.apiHost                 # the host in use
octocode config unset classification.apiHost               # back to the default
octocode config list                                       # every setting: value, source, default
```

On a terminal, `octocode config` opens a private local page for settings, keys and agent entries. Restart the MCP server or start a new session after any change, then confirm with a call that uses the key. [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md)
