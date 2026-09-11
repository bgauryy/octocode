# Awareness configuration

Awareness uses package defaults when `.octocode/awareness.json` is absent. Create or change workspace policy only when storage or host lifecycle behavior must differ.

## Workspace policy

The policy has version `1` and two sections:

```json
{
  "version": 1,
  "storage": {
    "repository": "global",
    "memory": "global"
  },
  "hooks": {
    "profile": "coordination",
    "owners": {
      "pi": "native"
    }
  }
}
```

`storage.repository` selects the store for Context, Work, Message, History, and host events. `storage.memory` selects the store for Memory. Each value is `global` or `repo`.

`hooks.profile` is `coordination`, `guard`, or `full`. `hooks.owners` can override the default `shell` or `native` owner for a supported host. Missing owner entries inherit defaults. Pi defaults to `native`; the other supported hosts default to `shell`.

The supported host keys are `claude`, `codex`, `cursor`, `copilot`, `gemini`, `opencode`, and `pi`.

The parser rejects unknown top-level, storage, hook, or host keys. It also rejects unsupported versions and values.

## Precedence

For a routine call, storage resolves in this order:

1. An explicit `--db` path.
2. An explicit client scope or `--db-scope` value.
3. The operation's workspace policy scope.
4. The global default.

A scope change does not copy or merge an existing store. All collaborators must use the same resolved database and normalized workspace identity.

## Lifecycle ownership

Select exactly one owner for each host. A native host must claim `native` ownership before lifecycle processing. When policy selects `native`, the shell runner exits before identity, database, or receipt work.

Configuration preference is not authorization to change a host configuration. Host installation and removal live outside the routine Awareness CLI.

For resolved locations, see [Storage scopes](STORAGE_SCOPES.md). For lifecycle semantics, see [Host hooks](HOOKS.md).
