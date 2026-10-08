import type { DatabaseSync } from 'node:sqlite';

/**
 * The agent database's schema: durable sessions, backlog and memories, and the team's transient state (presence,
 * messages kept a day, leases, recent edits). It migrates forward in ordered steps keyed by `user_version`; a file that
 * is not ours, or was written by a newer Octocode, is refused and never dropped.
 */

type Row = Record<string, unknown>;
const num = (value: unknown) => (typeof value === 'number' ? value : 0);

/** The file is not an agent database this code may use (another app's file, or a newer schema). Not retried. */
export class AgentDbError extends Error {}

/** Marks the file as an Octocode for Pi agent database ("OcPg"); a file with another id is never touched. */
export const AGENT_APPLICATION_ID = 0x4f635067;
export const AGENT_SCHEMA_VERSION = 1;

/** v1, the first published schema: sessions, backlog, memories and the team tables. */
const V1 = `
CREATE TABLE sessions (
  id TEXT PRIMARY KEY, branch TEXT, head TEXT,
  cost REAL NOT NULL DEFAULT 0, tokens INTEGER NOT NULL DEFAULT 0, files_changed INTEGER NOT NULL DEFAULT 0, pid INTEGER);

CREATE TABLE backlog (
  id INTEGER PRIMARY KEY AUTOINCREMENT, repo_key TEXT NOT NULL, seq INTEGER NOT NULL,
  title TEXT NOT NULL, body TEXT NOT NULL DEFAULT '',
  state TEXT NOT NULL CHECK(state IN ('backlog','todo','ongoing','done')),
  priority INTEGER NOT NULL DEFAULT 2 CHECK(priority BETWEEN 0 AND 3), tags TEXT NOT NULL DEFAULT '',
  assignee TEXT, created_by TEXT NOT NULL, source_session TEXT,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, done_at INTEGER, version INTEGER NOT NULL DEFAULT 1,
  UNIQUE(repo_key, seq));
CREATE INDEX backlog_board ON backlog(repo_key, state, priority, updated_at);
CREATE TABLE backlog_notes (
  id INTEGER PRIMARY KEY AUTOINCREMENT, item INTEGER NOT NULL REFERENCES backlog(id) ON DELETE CASCADE,
  at INTEGER NOT NULL, author TEXT NOT NULL, text TEXT NOT NULL);
CREATE INDEX backlog_notes_item ON backlog_notes(item, at);

CREATE TABLE memories (
  id INTEGER PRIMARY KEY AUTOINCREMENT, scope TEXT NOT NULL CHECK(scope IN ('global','project')),
  repo_key TEXT,
  kind TEXT NOT NULL DEFAULT 'fact' CHECK(kind IN ('preference','fact','decision','gotcha','procedure')),
  title TEXT NOT NULL, body TEXT NOT NULL DEFAULT '', keywords TEXT NOT NULL DEFAULT '',
  pinned INTEGER NOT NULL DEFAULT 0, author TEXT NOT NULL, source_session TEXT,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, last_used INTEGER, use_count INTEGER NOT NULL DEFAULT 0);
CREATE INDEX memories_scope ON memories(scope, repo_key, updated_at DESC);
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE agents (
  id TEXT PRIMARY KEY, workspace TEXT NOT NULL, role TEXT NOT NULL, parent_id TEXT, pid INTEGER NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('idle','working')), task TEXT, activity TEXT,
  joined_at INTEGER NOT NULL, seen_at INTEGER NOT NULL,
  tool_calls INTEGER NOT NULL DEFAULT 0, input INTEGER NOT NULL DEFAULT 0, output INTEGER NOT NULL DEFAULT 0, cost REAL NOT NULL DEFAULT 0,
  model TEXT);
CREATE INDEX agents_scope ON agents(workspace, joined_at);
CREATE TABLE messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT, workspace TEXT NOT NULL, sender TEXT NOT NULL, body TEXT NOT NULL,
  reply_required INTEGER NOT NULL DEFAULT 1, reply_to INTEGER REFERENCES messages(id) ON DELETE SET NULL,
  created_at INTEGER NOT NULL);
CREATE INDEX messages_age ON messages(created_at);
CREATE TABLE deliveries (
  message INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE, recipient TEXT NOT NULL,
  delivered_at INTEGER, completed_at INTEGER, dead_at INTEGER, wake INTEGER, notified_at INTEGER, PRIMARY KEY(message, recipient));
CREATE INDEX deliveries_inbox ON deliveries(recipient, delivered_at);
CREATE INDEX deliveries_unnotified ON deliveries(message) WHERE dead_at IS NOT NULL AND notified_at IS NULL;
CREATE TABLE leases (
  id INTEGER PRIMARY KEY AUTOINCREMENT, workspace TEXT NOT NULL, path TEXT NOT NULL, path_key TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('file','tree')), owner TEXT NOT NULL, reason TEXT NOT NULL,
  acquired_at INTEGER NOT NULL, expires_at INTEGER NOT NULL);
CREATE INDEX leases_path ON leases(workspace, path_key);
CREATE INDEX leases_owner ON leases(owner);
CREATE TABLE edits (
  id INTEGER PRIMARY KEY AUTOINCREMENT, workspace TEXT NOT NULL, path TEXT NOT NULL, agent TEXT NOT NULL, tool TEXT NOT NULL, at INTEGER NOT NULL);
CREATE INDEX edits_recent ON edits(workspace, at);
`;

/** Step `i` moves a file from `user_version` i to i + 1. Append only once published; never edit a shipped step. */
const MIGRATIONS: ReadonlyArray<(db: DatabaseSync) => void> = [(db) => db.exec(V1)];

const FTS = `
CREATE VIRTUAL TABLE memories_fts USING fts5(title, keywords, body, content='memories', content_rowid='id', tokenize='porter unicode61');
CREATE TRIGGER memories_fts_insert AFTER INSERT ON memories BEGIN
  INSERT INTO memories_fts(rowid, title, keywords, body) VALUES (new.id, new.title, new.keywords, new.body);
END;
CREATE TRIGGER memories_fts_delete AFTER DELETE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, title, keywords, body) VALUES ('delete', old.id, old.title, old.keywords, old.body);
END;
CREATE TRIGGER memories_fts_update AFTER UPDATE OF title, keywords, body ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, title, keywords, body) VALUES ('delete', old.id, old.title, old.keywords, old.body);
  INSERT INTO memories_fts(rowid, title, keywords, body) VALUES (new.id, new.title, new.keywords, new.body);
END;
INSERT INTO memories_fts(memories_fts) VALUES ('rebuild');
`;

const pragma = (db: DatabaseSync, name: string) => num((db.prepare(`PRAGMA ${name}`).get() as Row | undefined)?.[name]);

/**
 * Brings `db` to `latest` (run it inside one transaction), refusing another app's file or a newer schema. Then makes
 * sure the memory full-text index exists when SQLite has FTS5; returns whether it does.
 */
export function ensureSchema(db: DatabaseSync, file: string, migrations = MIGRATIONS, latest = AGENT_SCHEMA_VERSION): { fts: boolean } {
  const applicationId = pragma(db, 'application_id');
  let version = 0;
  if (applicationId === AGENT_APPLICATION_ID) {
    version = pragma(db, 'user_version');
    if (version > latest) throw new AgentDbError(`${file} holds agent state of schema ${version}, written by a newer Octocode (this one knows ${latest}); upgrade, or set OCTOCODE_AGENT_DB to another file.`);
  } else {
    const tables = db.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' LIMIT 1").get();
    if (applicationId !== 0 || tables !== undefined) throw new AgentDbError(`${file} is not an Octocode agent database; set OCTOCODE_AGENT_DB to another file.`);
    db.exec(`PRAGMA application_id=${AGENT_APPLICATION_ID}`);
  }
  for (; version < latest; version += 1) {
    migrations[version]!(db);
    db.exec(`PRAGMA user_version=${version + 1}`);
  }
  return { fts: ensureFts(db) };
}

/** The optional FTS5 index over memories (external content, kept in sync by triggers). False when FTS5 is missing. */
function ensureFts(db: DatabaseSync): boolean {
  if (db.prepare("SELECT 1 FROM sqlite_master WHERE name = 'memories_fts'").get() !== undefined) return true;
  db.exec('SAVEPOINT fts');
  try {
    db.exec(FTS);
    db.exec('RELEASE fts');
    return true;
  } catch {
    db.exec('ROLLBACK TO fts');
    db.exec('RELEASE fts');
    return false;
  }
}
