# Clean vendor sessions

Checked September 24, 2026 on Claude Code 2.1.281, Codex 0.155.0-alpha.9.2
and Pi 0.87.1. Here, clean means no inherited repository instructions, discovered
skills or user MCP servers. It does not mean no vendor system context or admin policy.
The communication proxy intentionally supplies its own skill and nine bound tools;
these standalone recipes do not run that proxy.

## Claude: tested with existing login

Start in an empty temporary directory outside the repository, then run:

```sh
claude -p --safe-mode --disable-slash-commands \
  --setting-sources '' --strict-mcp-config --mcp-config '{"mcpServers":{}}' \
  --tools '' --system-prompt 'Answer the user directly.' \
  --no-session-persistence --model haiku 'Your task'
```

The final live stream reported `tools: []`, `skills: []`, `mcp_servers: []`.
Only vendor built-in `agents-md` and `telemetry` plugin metadata remained.
The model returned `CLEAN_OK` with 418 input tokens. A temporary `CLAUDE.md`
and `AGENTS.md` containing a unique instruction marker did not affect the response.
This is a fixture observation, not proof of universal instruction isolation.

Safe mode alone still listed built-in skills; empty setting sources also removed
an installed Rust plugin from init metadata. Use the complete recipe above.
`--tools ''` makes this a text-only worker. A communication worker needs explicit
tools, and our earlier live safe-mode trial disabled even the supplied MCP bridge.
Do not automatically add safe mode to the existing communication adapter.

`--bare` is a different mode: this installed version skips OAuth/keychain reads.
Use it only with its supported API-key/provider authentication; it is not a drop-in
replacement for the existing logged-in session. No credentials were copied or changed.

## Codex: explicit isolation, no single clean flag

Use an empty cwd and a fresh `exec --ephemeral --ignore-user-config
--skip-git-repo-check --sandbox read-only` invocation. Authentication still uses
the existing Codex home. Add these per-run controls:

```text
--disable plugins --disable apps --disable hooks --disable memories
--disable multi_agent --disable shell_tool --disable skill_search
-c project_doc_max_bytes=0
-c web_search="disabled"
```

Quote the final TOML assignment as `-c 'web_search="disabled"'` in a shell.
These controls alone do not disable every discovered skill. Enumerate skills using
App Server `skills/list` with the isolated cwd and `forceReload: true`, then pass
one `skills.config` array containing `{path = "<returned path>", enabled = false}`
for each unique path. Pass that same array to `exec` with `-c`; do not write the
user's config. Recheck discovery when installed skills change.

The live protocol check changed **31 enabled skills to zero**. An `exec` invocation
with the same overrides returned `CLEAN_OK`, omitted the repository marker, and
reported 8,726 input tokens, including 3,840 cached tokens. Unlike Claude's custom
system-prompt run, this used Codex's default base instructions; the token counts
are not a like-for-like vendor benchmark. Remaining built-in tools were not
enumerated by this exec audit, so this is not a proven tool-free Codex session.

Do not rely on `skills.max_context_tokens=1`: a catalog budget is not disablement.
The installed experimental `skip_host_skill_discovery` feature also left all 31
skills enabled in `skills/list`. It is insufficient evidence for a no-skills claim.
`--ignore-user-config` is an exec option, not an App Server option; the existing
App Server adapter must still explicitly disable inherited MCP and plugin settings.
Profiles layer over user config and are not isolation. Admin/system configuration
may remain in force; no broad OS security boundary is provided by these controls.

## Pi: installed CLI controls

```sh
pi -p --no-session --no-context-files --no-skills --no-extensions \
  --no-prompt-templates --no-tools \
  --system-prompt 'Answer the user directly.' 'Your task'
```

Select an authenticated provider/model if the default is unsuitable. These flags
were checked against installed help; this audit did not make a fresh Pi model call.
Avoid explicit `--extension`/`--skill` additions when a clean session is intended.

## Evidence and sources

- [Final live results](../out/clean-explicit-check.json): Claude init inventory,
  usage and response; Codex enabled-skill count, usage and response.
- [Initial discovery probe](../out/clean-skills-check.json): experimental switch
  comparison. [Initial model probe](../out/clean-agent-check.json).
- [Claude CLI reference](https://code.claude.com/docs/en/cli-reference).
- [Codex exec flags](https://learn.chatgpt.com/docs/developer-commands).
- [Codex per-skill configuration](https://learn.chatgpt.com/docs/config-file/config-reference).

All probes used disposable directories and terminated their owned vendor processes.
No changes to vendor authentication, saved configuration, or Awareness were made.
