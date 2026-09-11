export const AWARENESS_META_DDL = `
    CREATE TABLE IF NOT EXISTS awareness_meta (
      application_id   INTEGER PRIMARY KEY,
      schema_version   INTEGER NOT NULL CHECK(schema_version > 0),
      store_id         TEXT NOT NULL UNIQUE CHECK(length(store_id) BETWEEN 36 AND 64),
      created_at       TEXT NOT NULL,
      last_migrated_at TEXT
    );
`;

/** Stable hook health receipts. Domain failures remain distinguishable from host exit. */
export const HOOK_RECEIPTS_DDL = `
    CREATE TABLE IF NOT EXISTS hook_receipts (
      workspace_path TEXT NOT NULL,
      host           TEXT NOT NULL CHECK(host IN ('claude','codex','copilot','cursor','gemini','opencode')),
      event          TEXT NOT NULL,
      status         TEXT NOT NULL CHECK(status IN ('success','failure')),
      last_seen_at   TEXT NOT NULL,
      PRIMARY KEY(workspace_path, host, event)
    );
`;
