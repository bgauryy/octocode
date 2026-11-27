Load when syncing a local skill you control to vendor (host) skill folders with skill-sync. CLI selection and destination folders: [install and fetch](install.md); failures: [recovery](install.md#recovery).

# Sync a local skill to vendors

- `node <this-skill>/scripts/skill-sync.mjs` symlinks each vendor destination to `<skill-dir>`: for a stable local skill you control, or on request; published skills use the install command.
- Read the dry-run plan (no `--approve`) first:

```bash
node <this-skill>/scripts/skill-sync.mjs <skill-dir> --platforms top
node <this-skill>/scripts/skill-sync.mjs <skill-dir> --platforms top --approve
node <this-skill>/scripts/skill-sync.mjs <skill-dir> --platforms claude,cursor --approve --force
node <this-skill>/scripts/skill-sync.mjs --list-vendors --json
```

- `--force` replaces conflicts; use it only when replacement is authorized. The script never prompts.
- `--project-root` adds project destinations **as well as user destinations**; inspect every row. For project-only installation use the Octocode CLI's `--project-dir`.
- `top` is `claude,cursor,codex`; `--list-vendors --json` lists every id `all` covers. Other options: `--help`.
