# Awareness storage scopes

Awareness supports global, repository, and explicit database selection. All actors that must coordinate need the same resolved physical database.

## Global scope

Global scope is the default. Its database is:

```text
$OCTOCODE_HOME/awareness/awareness.sqlite3
```

If `OCTOCODE_HOME` is unset, the platform Octocode home supplies the base directory.

## Repository scope

Repository scope stores the database at:

```text
<workspace>/.octocode/awareness.sqlite3
```

Select it through workspace policy, a client scope, or `--db-scope repo`. Repository scope is local to the physical workspace path unless host discovery intentionally joins linked worktrees through the same store.

## Explicit database

`--db <path>` or `AwarenessClientContext.database` selects one exact database. Use an absolute path in host integrations and migrations.

An explicit path does not bypass schema fingerprint checks. It also does not merge data from the global or repository store.

## Split repository and Memory scopes

Workspace policy can select `storage.repository` and `storage.memory` independently. Context, Work, Message, History, and host events use the repository choice. `memory.recall` and `memory.record` use the Memory choice.

Keep a split only when the different retention and sharing boundary is intentional. Follow a continuation under the same resolved scope as its originating call.

## LocalGit namespace

LocalGit is rooted in the workspace rather than beside the database. A stable store ID and physical workspace identity partition its private namespace. `history.status` returns the resolved root and Git directory.

Do not move, derive, or combine LocalGit namespaces independently of their SQLite metadata. Back up and migrate them as one evidence set.

For policy fields, see [Configuration](CONFIGURATION.md). For schema ownership, see [Awareness database](DB.md).
