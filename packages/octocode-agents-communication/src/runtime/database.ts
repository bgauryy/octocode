/** SQLite protocol storage and compatibility checks on the built-in `node:sqlite`; `database-export.ts` writes snapshots. */
import { createHash } from 'node:crypto';
import { closeSync, existsSync, mkdirSync, openSync, realpathSync } from 'node:fs';
import { DatabaseSync, type SQLOutputValue, type StatementSync } from 'node:sqlite';
import { basename, dirname, join, resolve } from 'node:path';
import schemaSql from '../../assets/schema.sql';
import { ValidationError } from './errors.js';
import { overlap, realpath } from './paths.js';
import { WHITESPACE_CLASS } from './text.js';
import { sleepSync, monotonic } from './timing.js';
import { compact } from './json-text.js';
import type { CommandResults } from './catalog-types.generated.js';

export const SQL: string = schemaSql;

const BUSY_TIMEOUT_MS = 5000;
const OPEN_RETRY_MS = 20;
const STATEMENT_CACHE = 128;

/** A value SQLite accepts. Booleans bind as 0/1 and whole numbers as INTEGER. */
export type SqlValue = string | number | bigint | boolean | null | Uint8Array;

/** A row as SQLite returns it: columns by name, each a number, bigint, string, bytes or null. */
export type SqlRow = Record<string, SQLOutputValue>;

/** A text column; any other type means the statement does not select what the caller reads. */
function text(value: SQLOutputValue, column: string): string {
  if (typeof value !== 'string') throw new TypeError(`SQLite column ${column} is not text`);
  return value;
}

/** An integer column (rows are read without BigInt, so INTEGER arrives as a number). */
function integer(value: SQLOutputValue | undefined, column: string): number {
  if (typeof value !== 'number') throw new TypeError(`SQLite column ${column} is not an integer`);
  return value;
}

function bind(value: SqlValue): string | number | bigint | null | Uint8Array {
  if (typeof value === 'boolean') return value ? 1n : 0n;
  if (typeof value === 'number' && Number.isSafeInteger(value)) return BigInt(value);
  // `SqlValue` excludes undefined; this guards callers that pass untyped rows.
  if (value === undefined) throw new TypeError('Cannot bind undefined to a SQLite parameter');
  return value;
}

/** A connection with `run`/`query`/`value` helpers; every write path goes through `transaction`. */
export class Database {
  private readonly statements = new Map<string, StatementSync>();

  constructor(readonly raw: DatabaseSync) {}

  get inTransaction(): boolean {
    return this.raw.isTransaction;
  }

  private statement(sql: string): StatementSync {
    let statement = this.statements.get(sql);
    if (statement) this.statements.delete(sql);
    else {
      statement = this.raw.prepare(sql);
      if (this.statements.size >= STATEMENT_CACHE) this.statements.delete(this.statements.keys().next().value!);
    }
    this.statements.set(sql, statement);
    return statement;
  }

  /** Runs statements that return no rows. */
  exec(sql: string): void {
    this.raw.exec(sql);
  }

  /** Runs one statement; `changes` is the affected row count and `lastInsertRowid` the new row ID. */
  run(sql: string, args: readonly SqlValue[] = []): { changes: number; lastInsertRowid: number } {
    const { changes, lastInsertRowid } = this.statement(sql).run(...args.map(bind));
    return { changes: Number(changes), lastInsertRowid: Number(lastInsertRowid) };
  }

  get(sql: string, args: readonly SqlValue[] = []): SqlRow | undefined {
    return this.statement(sql).get(...args.map(bind));
  }

  all(sql: string, args: readonly SqlValue[] = []): SqlRow[] {
    return this.statement(sql).all(...args.map(bind));
  }

  /** The first column of the first row, or undefined. */
  value(sql: string, args: readonly SqlValue[] = []): SQLOutputValue | undefined {
    const row = this.get(sql, args);
    return row === undefined ? undefined : Object.values(row)[0];
  }

  /** The first column of the first row as a number; fails when the statement returns another type or no row. */
  integer(sql: string, args: readonly SqlValue[] = []): number {
    return integer(this.value(sql, args), 'first column');
  }

  close(): void {
    this.statements.clear();
    this.raw.close();
  }
}

/** A row without its NULL columns; its values are text, whole numbers or (for `replyRequired`) booleans. */
export type Row = { [column: string]: string | number | boolean | undefined };

/** Rows as plain objects without their NULL columns; `replyRequired` is a boolean. Statements select no BLOB columns. */
export function query(db: Database, sql: string, args: readonly SqlValue[] = []): Row[] {
  return db.all(sql, args).map(row => {
    const result: Row = {};
    for (const [key, value] of Object.entries(row)) {
      if (value === null) continue;
      if (typeof value === 'bigint' || value instanceof Uint8Array) throw new TypeError(`SQLite column ${key} is not text or a number`);
      result[key] = key === 'replyRequired' ? Boolean(value) : value;
    }
    return result;
  });
}

/** Readers of one column of a `query` row: each checks its column once, so schema drift fails loudly instead of reading `undefined`. */
export function rowText(row: Row, column: string): string {
  const value = row[column];
  if (typeof value !== 'string') throw new TypeError(`SQLite column ${column} is not text`);
  return value;
}

export function rowInteger(row: Row, column: string): number {
  const value = row[column];
  if (typeof value !== 'number') throw new TypeError(`SQLite column ${column} is not an integer`);
  return value;
}

/** The column as text, or undefined when it was NULL. */
export function rowOptionalText(row: Row, column: string): string | undefined {
  return row[column] === undefined ? undefined : rowText(row, column);
}

/** The column as an integer, or undefined when it was NULL. */
export function rowOptionalInteger(row: Row, column: string): number | undefined {
  return row[column] === undefined ? undefined : rowInteger(row, column);
}

/** The number of rows a statement changed. */
export function execute(db: Database, sql: string, args: readonly SqlValue[] = []): number {
  return db.run(sql, args).changes;
}

function within<T>(db: Database, begin: string, body: () => T): T {
  db.exec(begin);
  try {
    const result = body();
    if (result instanceof Promise) throw new TypeError('A transaction body must be synchronous');
    db.exec('COMMIT');
    return result;
  } catch (error) {
    if (db.inTransaction) {
      try {
        db.exec('ROLLBACK');
      } catch {
        /* the original error matters more */
      }
    }
    throw error;
  }
}

/** Runs `body` inside `BEGIN IMMEDIATE … COMMIT`, rolling back when it throws. */
export function transaction<T>(db: Database, body: () => T): T {
  return within(db, 'BEGIN IMMEDIATE', body);
}

/** Runs `body` inside one deferred read snapshot. */
export function readTransaction<T>(db: Database, body: () => T): T {
  return within(db, 'BEGIN DEFERRED', body);
}

let engineVersion: number[] | undefined;

/** The linked SQLite version as [major, minor, patch]. */
export function sqliteVersion(): number[] {
  if (!engineVersion) {
    const memory = new DatabaseSync(':memory:');
    try {
      const row = memory.prepare('SELECT sqlite_version() AS version').get();
      engineVersion = text(row?.version ?? null, 'version').split('.').map(Number);
    } finally {
      memory.close();
    }
  }
  return engineVersion;
}

interface Metadata {
  applicationId: number;
  schemaVersion: number;
  sqliteVersion: string;
  journalMode: string;
}

export function metadata(db: Database): Metadata {
  const pragma = (name: string): SQLOutputValue | undefined => db.value('PRAGMA ' + name);
  return { applicationId: integer(pragma('application_id'), 'application_id'), schemaVersion: integer(pragma('user_version'), 'user_version'), sqliteVersion: sqliteVersion().join('.'), journalMode: text(pragma('journal_mode') ?? null, 'journal_mode') };
}

/** Whitespace runs collapse to one space. */
const WHITESPACE_RUN = new RegExp(`[${WHITESPACE_CLASS}]+`);

function collapse(text: string): string {
  return text.split(WHITESPACE_RUN).filter(Boolean).join(' ');
}

/** SHA-256 of the whitespace-normalized schema objects; persisted databases compare against it. */
export function fingerprint(db: Database): string {
  const rows = db.all("SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name").map(row => ({ type: row.type, name: row.name, tbl_name: row.tbl_name, sql: collapse(row.sql === null ? '' : text(row.sql, 'sql')) }));
  return createHash('sha256').update(compact(rows)).digest('hex');
}

interface Relationship {
  table: string;
  columns: string[];
  references: { table: string; columns: string[] };
  onDelete: string;
}

interface SchemaIdentity {
  applicationId: number;
  schemaVersion: number;
  schemaSha256: string;
  relationships: Relationship[];
}

let identity: SchemaIdentity | undefined;

/** Identity of the bundled schema.sql: application id, version, fingerprint and foreign keys. */
export function schema(): SchemaIdentity {
  if (!identity) {
    const db = new Database(new DatabaseSync(':memory:'));
    try {
      db.exec(SQL);
      const info = metadata(db);
      const relationships: Relationship[] = [];
      for (const { name } of db.all("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")) {
        const keys = new Map<number, Relationship>();
        for (const key of db.all('SELECT id,seq,"table","from","to",on_delete FROM pragma_foreign_key_list(?) ORDER BY id,seq', [text(name, 'name')])) {
          const id = integer(key.id, 'id');
          let relation = keys.get(id);
          if (!relation) keys.set(id, (relation = { table: text(name, 'name'), columns: [], references: { table: text(key.table, 'table'), columns: [] }, onDelete: text(key.on_delete, 'on_delete') }));
          relation.columns.push(text(key.from, 'from'));
          relation.references.columns.push(text(key.to, 'to'));
        }
        relationships.push(...keys.values());
      }
      identity = { applicationId: info.applicationId, schemaVersion: info.schemaVersion, schemaSha256: fingerprint(db), relationships };
    } finally {
      db.close();
    }
  }
  return identity;
}

export function compatible(db: Database, info: Metadata = metadata(db)): boolean {
  const current = schema();
  return info.applicationId === current.applicationId && info.schemaVersion === current.schemaVersion && fingerprint(db) === current.schemaSha256;
}

export function validate(db: Database, info?: Metadata): void {
  if (!compatible(db, info)) throw new ValidationError('Incompatible agents-communication database: its schema fingerprint differs from this runtime. Use a fresh --database; existing data is not modified.');
}

/** Describes a database file without modifying it. */
export function inspect(path: string, workspace: string): CommandResults['db info'] {
  const current = schema();
  const result: CommandResults['db info'] = {
    path,
    workspace: realpath(workspace, true),
    exists: existsSync(path),
    expectedApplicationId: current.applicationId,
    expectedSchemaVersion: current.schemaVersion,
    expectedSchemaSha256: current.schemaSha256,
    compatible: false,
  };
  if (result.exists) {
    checkPathLength(path);
    const db = new Database(new DatabaseSync(realpathSync(path), { readOnly: true }));
    try {
      Object.assign(result, metadata(db), { schemaSha256: fingerprint(db), compatible: compatible(db) });
    } finally {
      db.close();
    }
  }
  return result;
}

/** Upstream WAL-reset fixes, including maintained backport branches. */
export function walSafe(version: readonly number[]): boolean {
  const [major = 0, minor = 0, patch = 0] = version;
  const at = (a: number, b: number, c: number): boolean => major > a || (major === a && (minor > b || (minor === b && patch >= c)));
  return at(3, 51, 3) || (at(3, 50, 7) && !at(3, 51, 0)) || (at(3, 44, 6) && !at(3, 45, 0));
}

/**
 * SQLite's unix VFS resolves every database path into a 512-byte buffer and the `-journal` name has to fit it as well, so the
 * longest path it opens is 504 bytes. Measured on Node 24.15.0 (SQLite 3.51.3) and Node 26.4.0 (SQLite 3.53.2): 504 bytes open and
 * write in rollback-journal and WAL mode, 505 fail with "unable to open database file". The resolved path counts, symlinks included.
 */
const MAX_DATABASE_PATH_BYTES = 504;

/** The path as SQLite sees it: absolute, with symlinks resolved as far as the path exists. */
function resolvedPath(path: string): string {
  let head = resolve(path);
  let tail = '';
  for (;;) {
    try {
      return join(realpathSync(head), tail);
    } catch {
      const parent = dirname(head);
      if (parent === head) return join(head, tail);
      tail = join(basename(head), tail);
      head = parent;
    }
  }
}

/** Refuses a database path SQLite cannot open, before any directory or file is created. */
export function checkPathLength(path: string): void {
  const bytes = Buffer.byteLength(resolvedPath(path));
  if (bytes > MAX_DATABASE_PATH_BYTES)
    throw new ValidationError(`Database path is ${bytes} bytes after resolving symlinks; SQLite opens at most ${MAX_DATABASE_PATH_BYTES} bytes. Use a shorter --database or --workspace path.`);
}

function busy(error: unknown): boolean {
  return /locked|busy/i.test(String((error as Error).message));
}

/** Opens (and, when `create`, initializes) a communication database; refuses a schema this runtime does not own. */
export function openDatabase(path: string, readOnly = false, create = false): Database {
  const version = sqliteVersion();
  if (version[0]! < 3 || (version[0] === 3 && version[1]! < 42)) throw new ValidationError('SQLite >=3.42.0 required; use a Node.js release linked to a newer SQLite');
  checkPathLength(path);
  if (!existsSync(path)) {
    if (readOnly || !create) throw new ValidationError(`Database does not exist: ${path}. Run join first.`);
    mkdirSync(dirname(path), { mode: 0o700, recursive: true });
    try {
      closeSync(openSync(path, 'wx', 0o600));
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error;
    }
  }
  const deadline = monotonic() + BUSY_TIMEOUT_MS;
  for (;;) {
    try {
      return openConnection(path, readOnly, create);
    } catch (error) {
      if (!busy(error) || monotonic() >= deadline) throw error;
      sleepSync(OPEN_RETRY_MS);
    }
  }
}

function openConnection(path: string, readOnly: boolean, create: boolean): Database {
  const version = sqliteVersion();
  const db = new Database(new DatabaseSync(path, { readOnly, timeout: BUSY_TIMEOUT_MS, enableForeignKeyConstraints: true }));
  try {
    db.raw.function('lease_overlap', { deterministic: true }, (a, ak, b, bk) => (overlap(a as string, ak as string, b as string, bk as string) ? 1 : 0));
    if (readOnly) {
      readTransaction(db, () => validate(db));
      return db;
    }
    let info = readTransaction(db, () => {
      const read = metadata(db);
      if (!(read.applicationId === 0 && read.schemaVersion === 0)) validate(db, read);
      return read;
    });
    const uninitialized = info.applicationId === 0 && info.schemaVersion === 0;
    let initializedHere = false;
    if (info.journalMode === 'wal' && !walSafe(version))
      throw new ValidationError('This database uses WAL; upgrade Node.js to a release linked to SQLite 3.51.3+, 3.50.7+, or 3.44.6+ on their respective release branches before writing. Existing journal mode is unchanged.');
    if (uninitialized) {
      transaction(db, () => {
        info = metadata(db);
        const empty = db.value('SELECT count(*) FROM sqlite_schema') === 0;
        if (empty && info.applicationId === 0 && info.schemaVersion === 0) {
          if (!create) throw new ValidationError('Database is not initialized. Run join first.');
          db.exec(SQL);
          initializedHere = true;
        } else validate(db, info);
      });
    }
    if (initializedHere && walSafe(version)) db.exec('PRAGMA journal_mode=WAL');
    db.exec('PRAGMA synchronous=FULL');
    return db;
  } catch (error) {
    db.close();
    throw error;
  }
}
