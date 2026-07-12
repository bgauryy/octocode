import { createHash } from 'node:crypto';
import type { TableInfoRow } from './types.js';
import { DatabaseSync, readSchemaIdentity } from './db-runtime.js';
import type { SchemaIdentity } from './db-runtime.js';
import { FTS_SCHEMA_DDL, SCHEMA_DDL, SCHEMA_INDEX_DDL } from './db-schema.js';

export function tableColumns(db: DatabaseSync, tableName: string): Set<string> {
  const rows = db.prepare(`PRAGMA table_info(${tableName})`).all() as unknown as TableInfoRow[];
  return new Set(rows.map((row) => row.name));
}

export interface ColumnInfo {
  name: string;
  type: string;
  notnull: number;
  dflt_value: string | null;
}

export let _canonicalColumns: Map<string, ColumnInfo[]> | undefined;

/** Desired columns per table, derived from the executable DDL. */
export function canonicalColumns(): Map<string, ColumnInfo[]> {
  if (_canonicalColumns) return _canonicalColumns;
  const canonical = new DatabaseSync(':memory:');
  try {
    canonical.exec(SCHEMA_DDL);
    const tables = canonical.prepare(
      "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    ).all() as unknown as Array<{ name: string }>;
    _canonicalColumns = new Map(tables.map(({ name }) => [
      name,
      canonical.prepare(`PRAGMA table_info(${name})`).all() as unknown as ColumnInfo[],
    ]));
    return _canonicalColumns;
  } finally {
    canonical.close();
  }
}

export function normalizeSchemaSql(sql: string): string {
  return sql
    .replace(/--[^\n]*/g, ' ')
    .replace(/["`\[\]]/g, '')
    .replace(/\bIF\s+NOT\s+EXISTS\b/gi, '')
    .replace(/\s+/g, ' ')
    .replace(/\s*([(),])\s*/g, '$1')
    .trim()
    .toLowerCase();
}

export interface SchemaObject {
  type: string;
  name: string;
  tableName: string;
  sql: string;
}

export function readSchemaObjects(db: DatabaseSync): SchemaObject[] {
  const rows = db.prepare(`
    SELECT type, name, tbl_name, sql
    FROM sqlite_schema
    WHERE type IN ('table', 'view', 'index', 'trigger')
      AND name NOT LIKE 'sqlite_%'
      AND name NOT GLOB 'memories_fts_*'
    ORDER BY type, name
  `).all() as Array<{ type: string; name: string; tbl_name: string; sql: string | null }>;
  return rows.map((row) => ({
    type: row.type,
    name: row.name,
    tableName: row.tbl_name,
    sql: normalizeSchemaSql(row.sql ?? ''),
  }));
}

export function schemaObjectsFingerprint(objects: SchemaObject[]): string {
  return createHash('sha256').update(JSON.stringify(objects)).digest('hex');
}

export const _canonicalSchemaFingerprints = new Map<boolean, string>();
export const _priorHookReceiptSchemaFingerprints = new Map<boolean, string>();

export function canonicalSchemaFingerprint(includeFts: boolean): string {
  const cached = _canonicalSchemaFingerprints.get(includeFts);
  if (cached) return cached;
  const canonical = new DatabaseSync(':memory:');
  try {
    canonical.exec(SCHEMA_DDL);
    canonical.exec(SCHEMA_INDEX_DDL);
    if (includeFts) canonical.exec(FTS_SCHEMA_DDL);
    const fingerprint = schemaObjectsFingerprint(readSchemaObjects(canonical));
    _canonicalSchemaFingerprints.set(includeFts, fingerprint);
    return fingerprint;
  } finally {
    canonical.close();
  }
}

/** Exact immediately-prior schema: canonical in every respect except the new receipt table. */
export function priorHookReceiptSchemaFingerprint(includeFts: boolean): string {
  const cached = _priorHookReceiptSchemaFingerprints.get(includeFts);
  if (cached) return cached;
  const prior = new DatabaseSync(':memory:');
  try {
    prior.exec(SCHEMA_DDL);
    prior.exec(SCHEMA_INDEX_DDL);
    if (includeFts) prior.exec(FTS_SCHEMA_DDL);
    prior.exec('DROP TABLE hook_receipts');
    const fingerprint = schemaObjectsFingerprint(readSchemaObjects(prior));
    _priorHookReceiptSchemaFingerprints.set(includeFts, fingerprint);
    return fingerprint;
  } finally {
    prior.close();
  }
}

export function isExactPriorHookReceiptSchema(
  db: DatabaseSync,
  relations?: SchemaIdentity['relations'],
): boolean {
  const actualRelations = relations ?? readSchemaIdentity(db).relations;
  const expected = new Set([...canonicalColumns().keys()].filter((name) => name !== 'hook_receipts'));
  const actual = actualRelations.filter(({ name }) => name !== 'memories_fts');
  if (actual.some(({ type }) => type !== 'table')) return false;
  if (actual.length !== expected.size || actual.some(({ name }) => !expected.has(name))) return false;
  const objects = readSchemaObjects(db);
  const includeFts = objects.some(({ type, name }) => type === 'table' && name === 'memories_fts');
  return schemaObjectsFingerprint(objects) === priorHookReceiptSchemaFingerprint(includeFts);
}

export function assertCanonicalRelationContract(
  db: DatabaseSync,
  relations?: SchemaIdentity['relations'],
): void {
  const actualRows = relations ?? readSchemaIdentity(db).relations;
  const expected = new Set(canonicalColumns().keys());
  const actual = new Set(actualRows.map(({ name }) => name));
  const missing = [...expected].filter((name) => !actual.has(name));
  const unexpected = actualRows.filter(({ name, type }) => (
    type !== 'table' || (!expected.has(name) && name !== 'memories_fts')
  ));
  if (missing.length === 0 && unexpected.length === 0) return;
  const details = [
    missing.length > 0 ? `missing: ${missing.join(', ')}` : null,
    unexpected.length > 0 ? `unexpected: ${unexpected.map(({ name }) => name).join(', ')}` : null,
  ].filter((value): value is string => value !== null).join('; ');
  throw new Error(`canonical relation contract mismatch (${details})`);
}

export function assertCanonicalSchemaFingerprint(db: DatabaseSync): void {
  const objects = readSchemaObjects(db);
  const includeFts = objects.some(({ type, name }) => type === 'table' && name === 'memories_fts');
  const expectedFingerprint = canonicalSchemaFingerprint(includeFts);
  const actualFingerprint = schemaObjectsFingerprint(objects);
  if (actualFingerprint !== expectedFingerprint) {
    throw new Error(
      `canonical schema fingerprint mismatch (expected ${expectedFingerprint}, got ${actualFingerprint})`,
    );
  }
}
