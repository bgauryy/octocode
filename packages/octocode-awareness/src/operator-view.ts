import { randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, realpathSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { DatabaseSync, SQLITE_BUSY_DEADLINE_MS } from './sqlite.js';
import { getDatabasePath } from './db-runtime.js';
import { assertDatabaseIntegrity, inspectSchemaState, stableIdentityHash } from './db-introspection.js';
import { historyStatus } from './history-query.js';
import { createHistoryContext } from './history-store.js';
import { renderOperatorAwarenessView, type AwarenessViewColumn, type AwarenessViewEntity, type AwarenessViewSnapshot } from './operator-view-html.js';
import { awarenessEntityCatalog } from './schema/entities.js';
import { agentRows } from './repo-coordination.js';

export interface AwarenessViewOptions {
  database: string;
  workspace: string;
  out?: string;
  open?: boolean;
  openFile?: (path: string) => void | Promise<void>;
}

function quoteIdentifier(value: string): string {
  return `"${value.replaceAll('"', '""')}"`;
}

function databaseMetadata(db: DatabaseSync) {
  const sqliteVersion = db.prepare('SELECT sqlite_version() AS value').get() as { value: string };
  const journalMode = db.prepare('PRAGMA journal_mode').get() as { journal_mode: string };
  const applicationId = db.prepare('PRAGMA application_id').get() as { application_id: number };
  const userVersion = db.prepare('PRAGMA user_version').get() as { user_version: number };
  return {
    path: getDatabasePath(db),
    sqlite_version: sqliteVersion.value,
    journal_mode: journalMode.journal_mode,
    application_id: Number(applicationId.application_id),
    user_version: Number(userVersion.user_version),
  };
}

function openExistingCanonicalDatabase(database: string): DatabaseSync {
  if (database === ':memory:') throw new Error('Awareness view requires an existing file-backed database.');
  if (!existsSync(database)) throw new Error(`Awareness view database does not exist: ${database}`);
  const db = new DatabaseSync(database, { readOnly: true });
  try {
    db.exec(`PRAGMA busy_timeout = ${SQLITE_BUSY_DEADLINE_MS}`);
    const state = inspectSchemaState(db);
    if (state !== 'canonical') {
      throw new Error(`Awareness view requires the current canonical store; found ${state}.`);
    }
    assertDatabaseIntegrity(db);
    return db;
  } catch (error) {
    db.close();
    throw error;
  }
}

function readEntities(db: DatabaseSync): AwarenessViewEntity[] {
  return awarenessEntityCatalog().entities.map(entity => {
    const columns = db.prepare(
      'SELECT cid, name, type, "notnull" AS not_null, dflt_value AS default_value, pk AS primary_key FROM pragma_table_info(?) ORDER BY cid',
    ).all(entity.name) as unknown as AwarenessViewColumn[];
    const rows = db.prepare(`SELECT * FROM ${quoteIdentifier(entity.name)} ORDER BY rowid`).all() as Array<Record<string, unknown>>;
    return { ...entity, columns, rows };
  });
}

function readAgents(db: DatabaseSync, workspace: string): AwarenessViewSnapshot['agents'] {
  const rows: AwarenessViewSnapshot['agents'] = [];
  const limit = 500;
  for (let offset = 0; ; offset += limit) {
    const page = agentRows(db, { workspacePath: workspace, limit, offset });
    rows.push(...page);
    if (page.length < limit) return rows;
  }
}

function snapshot(db: DatabaseSync, workspace: string): AwarenessViewSnapshot {
  db.exec('BEGIN');
  try {
    const result: AwarenessViewSnapshot = {
      generated_at: new Date().toISOString(),
      workspace,
      database: databaseMetadata(db),
      local_git: historyStatus(createHistoryContext(db, workspace, { readOnly: true })),
      agents: readAgents(db, workspace),
      entities: readEntities(db),
    };
    db.exec('COMMIT');
    return result;
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* transaction already ended */ }
    throw error;
  }
}

function defaultOutputPath(database: string, workspace: string): string {
  const base = database === ':memory:' ? join(tmpdir(), 'octocode-awareness') : join(dirname(database), 'views');
  return join(base, `awareness-${stableIdentityHash(workspace).slice(0, 16)}.html`);
}

function writePrivateFile(path: string, content: string): void {
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  const temporary = `${path}.${process.pid}.${randomUUID()}.tmp`;
  try {
    writeFileSync(temporary, content, { encoding: 'utf8', mode: 0o600 });
    renameSync(temporary, path);
  } finally {
    try { unlinkSync(temporary); } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    }
  }
}

export async function openLocalAwarenessView(path: string): Promise<void> {
  const [command, args] = process.platform === 'darwin'
    ? ['open', [path]]
    : process.platform === 'win32'
      ? ['explorer.exe', [path]]
      : ['xdg-open', [path]];
  await new Promise<void>((resolvePromise, reject) => {
    const child = spawn(command, args, { detached: true, stdio: 'ignore' });
    child.once('error', reject);
    child.once('spawn', () => { child.unref(); resolvePromise(); });
  });
}

export async function createOperatorAwarenessView(options: AwarenessViewOptions) {
  const workspace = realpathSync(resolve(options.workspace));
  const database = options.database === ':memory:' ? options.database : resolve(options.database);
  const output = options.out
    ? (isAbsolute(options.out) ? resolve(options.out) : resolve(workspace, options.out))
    : defaultOutputPath(database, workspace);
  const db = openExistingCanonicalDatabase(database);
  let view: AwarenessViewSnapshot;
  try { view = snapshot(db, workspace); }
  finally { db.close(); }
  writePrivateFile(output, renderOperatorAwarenessView(view));

  let opened = false;
  let openError: string | undefined;
  if (options.open !== false) {
    try {
      await (options.openFile ?? openLocalAwarenessView)(output);
      opened = true;
    } catch (error) {
      openError = error instanceof Error ? error.message : String(error);
    }
  }
  return {
    ok: true as const,
    kind: 'awareness.view' as const,
    path: output,
    opened,
    ...(openError ? { open_error: openError } : {}),
    database,
    workspace,
    entity_count: view.entities.length,
    agent_count: view.agents.length,
    row_count: view.entities.reduce((sum, entity) => sum + entity.rows.length, 0),
    local_git: {
      initialized: view.local_git['initialized'] === true,
      operations: Number(view.local_git['operations'] ?? 0),
    },
  };
}
