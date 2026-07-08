<octocode_cli>
The Octocode CLI is **bundled** inside the extension and available as `$OCTOCODE_CLI` (set at startup).
Run it with `node`: `bash: node $OCTOCODE_CLI <command>`

`node $OCTOCODE_CLI` is the **bundled equivalent of `npx octocode`** — same commands and flags, no separate installation needed.

**Archive unpacking** — unpack an archive to a local dir, then research it with local tools.
```
bash: node $OCTOCODE_CLI unzip path/to/archive.zip
# returns localPath → then use localViewStructure, localSearchCode, localGetFileContent
```

**Cache — materialize GitHub content locally** — fetch repos/files into local cache for local-tool research.
```
bash: node $OCTOCODE_CLI cache fetch owner/repo [path]        # materialize a repo or subtree
bash: node $OCTOCODE_CLI cache fetch owner/repo@branch [path] # specific branch
bash: node $OCTOCODE_CLI cache status                         # see what is cached
bash: node $OCTOCODE_CLI cache clear --all                    # clear all cached data
```

**Install / manage skills** — install agent skills into supported local skill directories.
```
bash: node $OCTOCODE_CLI skill --list                                      # discover available skills
bash: node $OCTOCODE_CLI skill --name octocode-research                    # install a named skill
bash: node $OCTOCODE_CLI skill --name octocode-research --platform pi      # pi-specific path
bash: node $OCTOCODE_CLI skill --add --path {{path_to_skills_location}} --platform pi # install from an agent-known bundled/local skills path
bash: node $OCTOCODE_CLI skill --add {{GITHUB_PATH_TO_SKILL}} --platform pi # install from a GitHub path
```

**Tool schema & direct runs** — read tool schemas before calling; run tools via CLI as a last resort.
```
bash: node $OCTOCODE_CLI tools                                # list all 14 tools
bash: node $OCTOCODE_CLI tools <name> --scheme                # read exact schema (never guess fields)
bash: node $OCTOCODE_CLI tools <name> --queries '<json>' --compact  # lean tool run
```

**Other key commands**
```
bash: node $OCTOCODE_CLI clone owner/repo[/path]  # materialize a repo subtree locally
bash: node $OCTOCODE_CLI context                  # show agent protocol + tool playbook
bash: node $OCTOCODE_CLI lsp-server list          # list/install LSP language servers
bash: node $OCTOCODE_CLI auth login               # authenticate with GitHub — USER ONLY
```

**When to use** — prefer native Pi tools for all code reads/searches; use `node $OCTOCODE_CLI` for archive unpacking, cache materialization, skill management, and schema lookups.
**Find path** — run `/octocode-status` to see the exact `bundled CLI:` path if `$OCTOCODE_CLI` is unset.
</octocode_cli>
