import { DatabaseSync } from 'node:sqlite';
import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { SCHEMA_DDL, SCHEMA_INDEX_DDL } from '../src/db-schema.js';
import { historyExamples, historySchemas, HISTORY_ROUTE_DESCRIPTORS } from '../src/schema/definitions-history.js';
import { assertCanonicalSchemaFingerprint } from '../src/db-introspection.js';
import { schemas } from '../src/schema/registry.js';
import { CLI_REQUIRED, cliAllowedFlags } from '../src/schema/cli-contract.js';

describe('local history canonical contracts', () => {
  it('creates the normalized history ledger in the Awareness database', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(SCHEMA_DDL);
    db.exec(SCHEMA_INDEX_DDL);
    const tables = db.prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name LIKE 'local_history_%' ORDER BY name").all();
    expect(tables).toEqual([
      { name: 'local_history_durability' },
      { name: 'local_history_operations' },
      { name: 'local_history_restores' },
      { name: 'local_history_versions' },
    ]);
    db.close();
  });

  it('keeps each public route strict and excludes derived commit ids from inputs', () => {
    for (const route of HISTORY_ROUTE_DESCRIPTORS) expect(historySchemas).toHaveProperty(route.schema);
    expect(historySchemas.history_capture.safeParse({
      workspace: '/repo', agent_id: 'agent', phase: 'before', file: ['src/a.ts'], before_commit_oid: 'derived',
    }).success).toBe(false);
    expect(historySchemas.history_capture.safeParse({
      workspace: '/repo', agent_id: 'agent', phase: 'before', operation_id: 'op_retry', file: ['src/a.ts'],
    }).success).toBe(true);
    expect(historySchemas.history_capture.safeParse({
      workspace: '/repo', agent_id: 'agent', phase: 'after', outcome: 'success', file: ['src/a.ts'],
    }).success).toBe(false);
    const captureJsonSchema = JSON.stringify(z.toJSONSchema(historySchemas.history_capture));
    expect(captureJsonSchema).toContain('"const":"before"');
    expect(captureJsonSchema).toContain('"const":"after"');
    expect(captureJsonSchema).toMatch(/"required":\["workspace","agent_id","phase","operation_id","outcome"\]/);
    expect(historySchemas.history_restore_apply.safeParse({
      workspace: '/repo', agent_id: 'agent', preview_id: 'preview_1', side: 'before',
    }).success).toBe(false);
    expect(historySchemas.history_retention_prune.safeParse({ workspace: '/repo', confirm: 'prune', limit: 2 }).success).toBe(true);
    expect(historySchemas.history_retention_prune.safeParse({ workspace: '/repo', limit: 2 }).success).toBe(false);
    expect(historySchemas.history_recovery.safeParse({ workspace: '/repo', action: 'reconcile' }).success).toBe(false);
    expect(historySchemas.history_recovery.safeParse({ workspace: '/repo', action: 'reconcile', confirm: 'reconcile' }).success).toBe(true);
    expect(historySchemas.history_evidence.safeParse({ workspace: '/repo', action: 'reclaim' }).success).toBe(false);
    expect(historySchemas.history_evidence.safeParse({ workspace: '/repo', action: 'reclaim', confirm: 'reclaim', grace_seconds: 3600 }).success).toBe(true);
  });

  it('rejects every prior fingerprint instead of recognizing a legacy conversion shape', () => {
    const previous = new DatabaseSync(':memory:');
    previous.exec(SCHEMA_DDL);
    previous.exec(SCHEMA_INDEX_DDL);
    previous.exec('DROP TABLE local_history_restores; DROP TABLE local_history_versions; DROP TABLE local_history_operations');
    expect(() => assertCanonicalSchemaFingerprint(previous)).toThrow(/fingerprint mismatch/);
    previous.close();
  });

  it('registers every history route from the canonical descriptors', () => {
    for (const route of HISTORY_ROUTE_DESCRIPTORS) {
      expect(schemas).toHaveProperty(route.schema);
      expect(CLI_REQUIRED[route.command]).toEqual(route.required);
      expect(cliAllowedFlags(route.command)).toEqual(route.allowed);
    }
    for (const [name, schema] of Object.entries(historySchemas)) {
      expect(schema.safeParse(historyExamples[name as keyof typeof historyExamples]).success, name).toBe(true);
    }
    expect(historySchemas.local_history_restore.safeParse({
      ...(historyExamples.local_history_restore as object), expected_json: '{}',
    }).success).toBe(false);
    expect(historySchemas.local_history_restore.safeParse({
      ...(historyExamples.local_history_restore as object), target_json: '{broken',
    }).success).toBe(false);
  });
});
