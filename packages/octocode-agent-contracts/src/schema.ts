/**
 * schema.ts — the agent/session-owned tables of the shared local store.
 *
 * Dependency-free on purpose: it imports NO `node:sqlite`, so any owner of a
 * connection (normally the shared `openOctocodeDb`) can create these Agent
 * tables without dragging the `node:sqlite` top-level
 * import — and its ExperimentalWarning dance — into their module graph.
 *
 * The connection is typed structurally (`SqliteLike`) so both `node:sqlite`'s
 * `DatabaseSync` and any compatible handle satisfy it.
 */

/** Minimal structural view of a node:sqlite connection this module needs. */
export interface SqliteLike {
  exec(sql: string): void;
  prepare(sql: string): { run(...params: unknown[]): unknown };
}

/** ASCII "OCTA": the database is exclusively owned by the Octocode agent. */
export const AGENT_APPLICATION_ID = 0x4f435441;

/** UTC timestamp in ISO-8601, matching the rest of the stores. */
export function utcNow(): string {
  return new Date().toISOString();
}

/**
 * Create the agent/session-owned tables. Idempotent (`IF NOT EXISTS`), so it is
 * safe to call on every Agent control-database process start. Awareness schema
 * initialization is intentionally separate and must target another file.
 */
export function initOctocodeSchema(db: SqliteLike): void {
  db.exec(`
    CREATE TABLE IF NOT EXISTS agent_schema_modules (
      module_key  TEXT PRIMARY KEY,
      version     INTEGER NOT NULL CHECK(version > 0),
      fingerprint TEXT NOT NULL,
      updated_at  TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS octocode_meta (
      key        TEXT PRIMARY KEY,
      value      TEXT NOT NULL,
      updated_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS agent_sessions (
      session_id     TEXT PRIMARY KEY,
      workspace_path TEXT,
      cwd            TEXT,
      created_at     TEXT NOT NULL,
      updated_at     TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_agent_sessions_workspace
      ON agent_sessions(workspace_path);
    CREATE TABLE IF NOT EXISTS mcp_server_overrides (
      scope_key  TEXT NOT NULL,
      server_key TEXT NOT NULL,
      enabled    INTEGER NOT NULL CHECK(enabled IN (0, 1)),
      updated_at TEXT NOT NULL,
      PRIMARY KEY (scope_key, server_key)
    );
    CREATE TABLE IF NOT EXISTS mcp_tool_overrides (
      scope_key  TEXT NOT NULL,
      server_key TEXT NOT NULL,
      tool_name  TEXT NOT NULL,
      enabled    INTEGER NOT NULL CHECK(enabled IN (0, 1)),
      updated_at TEXT NOT NULL,
      PRIMARY KEY (scope_key, server_key, tool_name)
    );
    CREATE TABLE IF NOT EXISTS skill_overrides (
      scope_key  TEXT NOT NULL,
      skill_key  TEXT NOT NULL,
      enabled    INTEGER NOT NULL CHECK(enabled IN (0, 1)),
      updated_at TEXT NOT NULL,
      PRIMARY KEY (scope_key, skill_key)
    );
    CREATE TABLE IF NOT EXISTS mcp_catalog_state (
      scope_key     TEXT PRIMARY KEY,
      config_digest TEXT NOT NULL,
      catalog_digest TEXT NOT NULL,
      guide_digest  TEXT NOT NULL,
      status         TEXT NOT NULL,
      error          TEXT,
      updated_at     TEXT NOT NULL
    );
  `);
}

export interface SessionRecord {
  sessionId: string;
  workspacePath?: string | null;
  cwd?: string | null;
}

/**
 * Register (or touch) a session row. Upsert keeps `created_at` stable while
 * advancing `updated_at`, giving a durable cross-session index keyed by
 * workspace.
 */
export function recordSession(db: SqliteLike, session: SessionRecord): void {
  const now = utcNow();
  db.prepare(
    `INSERT INTO agent_sessions (session_id, workspace_path, cwd, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?)
     ON CONFLICT(session_id) DO UPDATE SET
       workspace_path = excluded.workspace_path,
       cwd            = excluded.cwd,
       updated_at     = excluded.updated_at`,
  ).run(session.sessionId, session.workspacePath ?? null, session.cwd ?? null, now, now);
}
