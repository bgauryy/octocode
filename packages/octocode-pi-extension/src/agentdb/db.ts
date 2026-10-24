import fs from 'node:fs';
import path from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { agentDbPath, globalPaths, privateDir } from '../shared/home.js';
import { loadSqlite } from '../shared/util.js';
import { AgentDbError, ensureSchema, schemaCurrent } from './schema.js';

/** An open agent database. Features write their own SQL against the schema tables (`./schema.ts`). */
export interface AgentDb {
  readonly file: string;
  readonly db: DatabaseSync;
  /** False when SQLite lacks FTS5: memory search falls back to LIKE. */
  readonly fts: boolean;
  /** One BEGIN IMMEDIATE transaction; nested calls join the outer one. */
  transaction<T>(fn: () => T): T;
  close(): void;
}

/**
 * Opens (creating, owner-only) the agent database at `file` (default `agentDbPath(env)`) and adds what the one schema has
 * and the file lacks. Throws `AgentDbError` for a file that is not ours.
 */
export function openAgentDb(file?: string, env: NodeJS.ProcessEnv = process.env): AgentDb {
  const target = file ?? agentDbPath(env);
  // Private to this user: it holds prompts, notes and memories. SQLite gives its -wal and -shm files the same mode.
  // Our own folder is tightened when it already exists; a folder named by OCTOCODE_AGENT_DB is only created.
  if (target === path.join(globalPaths(env).agentState, 'octocode.db')) privateDir(path.dirname(target));
  else fs.mkdirSync(path.dirname(target), { recursive: true, mode: 0o700 });
  fs.closeSync(fs.openSync(target, 'a', 0o600));
  const { DatabaseSync: Database } = loadSqlite();
  const db = new Database(target);
  let depth = 0;
  const transaction = <T>(fn: () => T): T => {
    if (depth > 0) return fn();
    db.exec('BEGIN IMMEDIATE');
    depth += 1;
    try {
      const result = fn();
      db.exec('COMMIT');
      return result;
    } catch (error) {
      try {
        db.exec('ROLLBACK');
      } catch {
        // SQLite already rolled back (some errors do); the failure to report is the original.
      }
      throw error;
    } finally {
      depth -= 1;
    }
  };
  try {
    // Nothing that changes the file (journal mode included) before ensureSchema has checked it is ours or new. A file
    // that already has the whole schema is opened without a write lock; otherwise the additions are made under one,
    // checked again there (another process may have just made them).
    db.exec('PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;');
    const { fts } = schemaCurrent(db) ? { fts: true } : transaction(() => ensureSchema(db, target));
    db.exec('PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;');
    tighten(target);
    let open = true;
    return {
      file: target,
      db,
      fts,
      transaction,
      close: () => {
        if (open) db.close();
        open = false;
      },
    };
  } catch (error) {
    db.close();
    if (isNotADatabase(error)) throw new AgentDbError(`${target} is not an SQLite database; set OCTOCODE_AGENT_DB to another file.`);
    throw error;
  }
}

/** A cheap change marker for the database file and its WAL: a write by any process moves it. Constant for a file that does not exist. */
export function dbStamp(file: string): string {
  return [file, `${file}-wal`]
    .map((entry) => {
      const stat = fs.statSync(entry, { throwIfNoEntry: false });
      return stat ? `${stat.mtimeMs}:${stat.size}` : '-';
    })
    .join('|');
}

/** SQLITE_NOTADB (26): the file has no SQLite header. */
const isNotADatabase = (error: unknown) => (error as { errcode?: number } | null)?.errcode === 26;

/** Owner-only modes for an existing database of ours and its WAL files (an older or copied file may be looser). */
function tighten(target: string): void {
  for (const each of [target, `${target}-wal`, `${target}-shm`]) {
    try {
      if ((fs.statSync(each).mode & 0o077) !== 0) fs.chmodSync(each, 0o600);
    } catch {
      // Missing, or not ours to change.
    }
  }
}

let shared: AgentDb | undefined;
const refused = new Map<string, AgentDbError>();

/**
 * The process's agent database, opened on first use (and reopened when `OCTOCODE_AGENT_DB` / `OCTOCODE_HOME` in `env`
 * now name another file). An `AgentDbError` is remembered per file and thrown again without retrying.
 */
export function sharedAgentDb(env: NodeJS.ProcessEnv = process.env): AgentDb {
  const file = agentDbPath(env);
  if (shared?.file === file) return shared;
  const problem = refused.get(file);
  if (problem) throw problem;
  shared?.close();
  shared = undefined;
  try {
    shared = openAgentDb(file, env);
  } catch (error) {
    if (error instanceof AgentDbError) refused.set(file, error);
    throw error;
  }
  return shared;
}

/** Closes the process's agent database (tests, process exit); the next `sharedAgentDb` opens it again. */
export function closeSharedAgentDb(): void {
  shared?.close();
  shared = undefined;
  refused.clear();
}
