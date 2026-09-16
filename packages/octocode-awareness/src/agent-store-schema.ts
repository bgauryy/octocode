import { createHash } from 'node:crypto';

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

export interface ReadableSqlite extends SqliteLike {
  prepare(sql: string): {
    run(...params: unknown[]): unknown;
    get(...params: unknown[]): unknown;
    all(...params: unknown[]): unknown[];
  };
}

export interface SchemaObject {
  type: string;
  name: string;
  tableName: string;
  sql: string;
}

/** Normalize SQL tokens without changing quoted identifiers or literal values. */
export function normalizeSchemaSql(sql: string): string {
  const tokens = sql.match(/--[^\n]*|\/\*[\s\S]*?\*\/|'(?:''|[^'])*'|"(?:""|[^"])*"|`(?:``|[^`])*`|\[[^\]]*\]|[a-zA-Z_][a-zA-Z_0-9$]*|\d+(?:\.\d+)?|!=|<>|<=|>=|==|\|\||->>|->|\S/g) ?? [];
  const normalized = tokens
    .filter(token => !token.startsWith('--') && !token.startsWith('/*'))
    .map(token => /^[\w]/.test(token) ? token.toLowerCase() : token);
  // SQLite removes this clause from sqlite_schema when it persists CREATE DDL.
  if (normalized[0] === 'create') {
    const kindIndex = ['unique', 'virtual', 'temp', 'temporary'].includes(normalized[1] ?? '') ? 2 : 1;
    const clause = kindIndex + 1;
    if (normalized.slice(clause, clause + 3).join(' ') === 'if not exists') normalized.splice(clause, 3);
  }
  if (normalized.at(-1) === ';') normalized.pop();
  return JSON.stringify(normalized);
}

/** Inspect only durable application objects; SQLite identifies its own shadow tables. */
export function readSchemaObjects(db: ReadableSqlite): SchemaObject[] {
  const shadows = new Set((db.prepare("SELECT name FROM pragma_table_list WHERE schema = 'main' AND type = 'shadow'").all() as Array<{ name: string }>).map(row => row.name));
  const rows = db.prepare(`SELECT type, name, tbl_name, sql FROM main.sqlite_schema
    WHERE type IN ('table', 'view', 'index', 'trigger') AND name NOT GLOB 'sqlite_*'
    ORDER BY type, name`).all() as Array<{ type: string; name: string; tbl_name: string; sql: string | null }>;
  return rows.filter(row => !shadows.has(row.tbl_name)).map(row => ({
    type: row.type, name: row.name, tableName: row.tbl_name, sql: normalizeSchemaSql(row.sql ?? ''),
  }));
}

export function schemaObjectsFingerprint(objects: SchemaObject[]): string {
  return createHash('sha256').update(JSON.stringify(objects)).digest('hex');
}

export function assertSchemaObjects(actual: SchemaObject[], expected: SchemaObject[]): void {
  const actualFingerprint = schemaObjectsFingerprint(actual);
  const expectedFingerprint = schemaObjectsFingerprint(expected);
  if (actualFingerprint === expectedFingerprint) return;
  const key = (item: SchemaObject) => `${item.type}:${item.name}`;
  const actualByKey = new Map(actual.map(item => [key(item), item]));
  const expectedByKey = new Map(expected.map(item => [key(item), item]));
  const missing = [...expectedByKey.keys()].filter(name => !actualByKey.has(name));
  const unexpected = [...actualByKey.keys()].filter(name => !expectedByKey.has(name));
  const changed = expected.filter(item => actualByKey.has(key(item)) && JSON.stringify(actualByKey.get(key(item))) !== JSON.stringify(item)).map(key);
  throw new Error(`canonical schema fingerprint mismatch (missing: ${missing.join(', ') || 'none'}; unexpected: ${unexpected.join(', ') || 'none'}; changed: ${changed.join(', ') || 'none'}; expected ${expectedFingerprint}, got ${actualFingerprint}). Select a current canonical store; the database has not been changed.`);
}

/** SQL enum literals are generated from the owning runtime/type contract. */
export function sqlEnum(values: readonly string[]): string {
  return values.map(value => `'${value.replaceAll("'", "''")}'`).join(',');
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
