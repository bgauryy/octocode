import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { z } from 'zod';
import { historyEntitySchemas, type HistoryInspectInput, type HistoryReadInput, type HistoryTimelineInput } from './schema/definitions-history.js';
import { HistoryError, historyHash, historyOperation, historyPath, historyStoragePaths, historyTransaction, historyVersions, type HistoryContext } from './history-store.js';
import { historyGitBackend } from './history-git.js';

const cursorSchema = z.object({ scope: z.string().length(64), before: z.number().int().positive() }).strict();
function continuation(ctx: HistoryContext, command: string, args: Record<string, unknown>) {
  const scoped = { ...args, workspace: ctx.requestWorkspace ?? ctx.workspace,
    ...(ctx.requestWorkspace ? { source_workspace: ctx.workspace } : {}) };
  const canonical = {
    timeline: { operation: 'history.timeline' },
    read: { operation: 'history.read' },
    'restore-preview': { operation: 'history.restore', action: 'preview' },
    'restore-apply': { operation: 'history.restore', action: 'apply' },
  }[command];
  if (canonical) {
    const { workspace: _workspace, ...params } = scoped;
    return {
      operation: canonical.operation,
      params: { ...params, ...(canonical.action ? { action: canonical.action } : {}) },
    };
  }
  const argv = ['history', command, '--db', ctx.dbPath];
  for (const [key, value] of Object.entries(scoped)) {
    if (value !== undefined) argv.push(`--${key.replaceAll('_', '-')}`, String(value));
  }
  argv.push('--compact');
  return { command: `history ${command}`, args: scoped, db: ctx.dbPath, argv };
}
export function historyStatus(ctx: HistoryContext) {
  const storage = historyStoragePaths(ctx);
  const count = ctx.db.prepare("SELECT COUNT(*) AS operations, SUM(status IN ('capturing','open','failed','partial')) AS incomplete, SUM(status='capturing') AS capturing FROM local_history_operations WHERE workspace_path=?").get(ctx.workspace)!;
  const restores = ctx.db.prepare("SELECT SUM(status='ready' AND expires_at<=?) AS expired, SUM(status='applying') AS applying FROM local_history_restores WHERE workspace_path=?")
    .get(new Date().toISOString(), ctx.workspace)!;
  return { ok: true, available: ctx.dbPath !== ':memory:', initialized: storage !== null && existsSync(join(storage.root, 'history-store.json')), storage,
    backend: historyGitBackend(),
    workspace: ctx.workspace, operations: Number(count.operations), incomplete: Number(count.incomplete ?? 0),
    retention: { automatic_object_pruning: false, capturing_operations: Number(count.capturing ?? 0),
      expired_restore_previews: Number(restores.expired ?? 0), applying_restores: Number(restores.applying ?? 0) },
    disabled_reason: ctx.dbPath === ':memory:' ? 'memory_database' : null };
}

function maintenanceCursor(ctx: HistoryContext, command: string, cursor?: string): number {
  if (!cursor) return Number.MAX_SAFE_INTEGER;
  try {
    const value = JSON.parse(Buffer.from(cursor, 'base64url').toString('utf8')) as { scope?: string; before?: number };
    if (value.scope !== historyHash(JSON.stringify([ctx.dbPath, ctx.workspace, command])) || !Number.isSafeInteger(value.before)) throw new Error();
    return Number(value.before);
  } catch { throw new HistoryError('HISTORY_CURSOR_INVALID', 'Maintenance cursor does not match this store and route.'); }
}

export function historyRetentionPreview(ctx: HistoryContext, input: import('./schema/definitions-history.js').HistoryRetentionPreviewInput) {
  const before = maintenanceCursor(ctx, 'retention-preview', input.cursor);
  const rows = ctx.db.prepare("SELECT rowid AS sequence, preview_id, source_operation_id, side, expires_at, created_at FROM local_history_restores WHERE workspace_path=? AND status='ready' AND expires_at<=? AND rowid<? ORDER BY rowid DESC LIMIT ?").all(ctx.workspace, new Date().toISOString(), before, input.limit + 1) as Array<Record<string, unknown>>;
  const page = rows.slice(0, input.limit);
  return { ok: true, dry_run: true, previews: page.map(({ sequence: _sequence, ...row }) => row), partial: rows.length > input.limit, next: rows.length > input.limit ? continuation(ctx, 'retention-preview', { limit: input.limit, cursor: Buffer.from(JSON.stringify({ scope: historyHash(JSON.stringify([ctx.dbPath, ctx.workspace, 'retention-preview'])), before: Number(page.at(-1)!.sequence) })).toString('base64url') }) : null };
}

export function historyRetentionPrune(ctx: HistoryContext, input: import('./schema/definitions-history.js').HistoryRetentionPruneInput) {
  const before = maintenanceCursor(ctx, 'retention-prune', input.cursor);
  const now = new Date().toISOString();
  return historyTransaction(ctx, () => {
    const rows = ctx.db.prepare("SELECT rowid AS sequence, preview_id, source_operation_id, side, expires_at, created_at FROM local_history_restores WHERE workspace_path=? AND status='ready' AND expires_at<=? AND rowid<? ORDER BY rowid DESC LIMIT ?").all(ctx.workspace, now, before, input.limit + 1) as Array<Record<string, unknown>>;
    const page = rows.slice(0, input.limit);
    for (const row of page) ctx.db.prepare("DELETE FROM local_history_restores WHERE preview_id=? AND status='ready' AND expires_at<=?").run(String(row.preview_id), now);
    return { ok: true, pruned: page.map(({ sequence: _sequence, ...row }) => row), partial: rows.length > input.limit, next: rows.length > input.limit ? continuation(ctx, 'retention-prune', { confirm: 'prune', limit: input.limit, cursor: Buffer.from(JSON.stringify({ scope: historyHash(JSON.stringify([ctx.dbPath, ctx.workspace, 'retention-prune'])), before: Number(page.at(-1)!.sequence) })).toString('base64url') }) : null };
  });
}

export function historyRecovery(ctx: HistoryContext, input: import('./schema/definitions-history.js').HistoryRecoveryInput) {
  let cursor = `${'9'.repeat(20)}:z`;
  if (input.cursor) {
    try {
      const decoded = JSON.parse(Buffer.from(input.cursor, 'base64url').toString('utf8')) as { scope?: string; sort_key?: string };
      if (decoded.scope !== historyHash(JSON.stringify([ctx.dbPath, ctx.workspace, 'recovery'])) || typeof decoded.sort_key !== 'string') throw new Error();
      cursor = decoded.sort_key;
    } catch { throw new HistoryError('HISTORY_CURSOR_INVALID', 'Recovery cursor does not match this store and workspace.'); }
  }
  const rows = ctx.db.prepare("SELECT rowid AS sequence, operation_id, kind, status, updated_at, printf('%020d:operation', rowid) AS sort_key FROM local_history_operations WHERE workspace_path=? AND status IN ('capturing','failed') AND printf('%020d:operation', rowid)<? UNION ALL SELECT rowid AS sequence, preview_id AS operation_id, 'restore' AS kind, status, created_at AS updated_at, printf('%020d:restore', rowid) AS sort_key FROM local_history_restores WHERE workspace_path=? AND status='applying' AND printf('%020d:restore', rowid)<? ORDER BY sort_key DESC LIMIT ?").all(ctx.workspace, cursor, ctx.workspace, cursor, input.limit + 1) as Array<Record<string, unknown>>;
  const page = rows.slice(0, input.limit);
  const reconciled: string[] = [];
  if (input.action === 'reconcile') {
    if (input.confirm !== 'reconcile') throw new HistoryError('HISTORY_INVALID_REQUEST', 'Reconciliation requires explicit confirmation.');
    for (const row of page) {
      if (row.kind === 'restore') {
        const raw = ctx.db.prepare('SELECT files_json,result_json,status FROM local_history_restores WHERE preview_id=?').get(String(row.operation_id)) as { files_json?: string; result_json?: string | null; status?: string } | undefined;
        let results: unknown[] | undefined;
        try {
          const journal: unknown = raw?.result_json ? JSON.parse(raw.result_json) : undefined;
          const candidate = journal && typeof journal === 'object' ? (journal as { results?: unknown }).results : undefined;
          if (Array.isArray(candidate)) results = candidate;
        } catch { results = undefined; }
        let files: string[] | undefined;
        try {
          const parsed = raw?.files_json ? JSON.parse(raw.files_json) : undefined;
          if (Array.isArray(parsed) && parsed.every(file => typeof file === 'string')) files = parsed;
        } catch { files = undefined; }
        const paths = results?.map(result => {
          if (!result || typeof result !== 'object') return null;
          const value = result as { path?: unknown; status?: unknown };
          return typeof value.path === 'string' && (value.status === 'restored' || value.status === 'deleted') ? value.path : null;
        });
        const exact = files && files.length > 0 && new Set(files).size === files.length
          && paths && paths.length === files.length && paths.every((path): path is string => path !== null)
          && new Set(paths).size === paths.length && files.every(file => paths.includes(file));
        if (raw?.status === 'applying' && exact) {
          ctx.db.prepare("UPDATE local_history_restores SET status='applied' WHERE preview_id=? AND status='applying'").run(String(row.operation_id));
          reconciled.push(String(row.operation_id));
        }
      }
    }
  }
  const nextCursor = rows.length > input.limit ? Buffer.from(JSON.stringify({ scope: historyHash(JSON.stringify([ctx.dbPath, ctx.workspace, 'recovery'])), sort_key: String(page.at(-1)!.sort_key) })).toString('base64url') : null;
  return { ok: true, action: input.action, reconciled, uncertain: page.filter(row => !reconciled.includes(String(row.operation_id))), partial: rows.length > input.limit, next: nextCursor ? continuation(ctx, 'recovery', { action: input.action, ...(input.action === 'reconcile' ? { confirm: 'reconcile' } : {}), limit: input.limit, cursor: nextCursor }) : null };
}

export async function historyEvidence(ctx: HistoryContext, input: import('./schema/definitions-history.js').HistoryEvidenceInput) {
  if (input.action === 'reclaim') return { ok: false, code: 'HISTORY_EVIDENCE_RECLAIM_UNAVAILABLE', diagnostic: 'Evidence reclamation is disabled until capture writers participate in one atomic maintenance handshake.', action: input.action, safety: 'report_only' };
  const store = await ctx.store();
  const rows = ctx.db.prepare('SELECT before_commit_oid AS oid FROM local_history_operations WHERE workspace_path=? AND before_commit_oid IS NOT NULL UNION SELECT after_commit_oid AS oid FROM local_history_operations WHERE workspace_path=? AND after_commit_oid IS NOT NULL').all(ctx.workspace, ctx.workspace) as Array<{ oid: string }>;
  const versions = ctx.db.prepare("SELECT before_oid AS oid FROM local_history_versions v JOIN local_history_operations o ON o.operation_id=v.operation_id WHERE o.workspace_path=? AND before_oid IS NOT NULL UNION SELECT after_oid AS oid FROM local_history_versions v JOIN local_history_operations o ON o.operation_id=v.operation_id WHERE o.workspace_path=? AND after_oid IS NOT NULL").all(ctx.workspace, ctx.workspace) as Array<{ oid: string }>;
  const result = await store.inspectOrphanObjects({ retainedOids: [...rows, ...versions].map(row => row.oid), graceMs: input.grace_seconds * 1000, limit: input.limit, cursor: input.cursor });
  return { ok: true, action: 'report' as const, dry_run: true, safety: 'observational', ...result, next: result.next_cursor ? continuation(ctx, 'evidence', { action: 'report', grace_seconds: input.grace_seconds, limit: input.limit, cursor: result.next_cursor }) : null };
}
export function historyTimeline(ctx: HistoryContext, input: HistoryTimelineInput) {
  const file = input.file === undefined ? undefined : historyPath(ctx, input.file);
  const scope = historyHash(JSON.stringify([ctx.dbPath, ctx.workspace, file ?? null]));
  let before = Number.MAX_SAFE_INTEGER;
  if (input.cursor) {
    try {
      const cursor = cursorSchema.parse(JSON.parse(Buffer.from(input.cursor, 'base64url').toString('utf8')));
      if (cursor.scope !== scope) throw new Error('scope mismatch');
      before = cursor.before;
    } catch { throw new HistoryError('HISTORY_CURSOR_INVALID', 'Timeline cursor does not match the selected store, workspace, and file.'); }
  }
  const clauses = ['o.workspace_path=?', 'o.rowid<?'];
  const bindings: Array<string | number> = [ctx.workspace, before];
  if (file) {
    clauses.push('EXISTS (SELECT 1 FROM local_history_versions v WHERE v.operation_id=o.operation_id AND v.file_path=?)');
    bindings.push(file);
  }
  const rows = ctx.db.prepare(`SELECT o.rowid AS sequence,o.*,
    (SELECT COUNT(*) FROM local_history_versions v WHERE v.operation_id=o.operation_id) AS file_count
    FROM local_history_operations o WHERE ${clauses.join(' AND ')} ORDER BY o.rowid DESC LIMIT ?`)
    .all(...bindings, input.limit + 1);
  const page = rows.slice(0, input.limit);
  const operations = page.map(({ sequence: _sequence, file_count, ...row }) => ({ ...historyEntitySchemas.local_history_operation.parse(row), file_count: Number(file_count) }));
  const last = page.at(-1);
  const cursor = last && rows.length > input.limit ? Buffer.from(JSON.stringify({ scope, before: Number(last.sequence) })).toString('base64url') : null;
  return { ok: true, operations, partial: cursor !== null, next: cursor ? continuation(ctx, 'timeline', { file, limit: input.limit, cursor }) : null };
}
export async function historyRead(ctx: HistoryContext, input: HistoryReadInput) {
  const file = historyPath(ctx, input.file);
  const row = historyVersions(ctx, input.operation_id).find(version => version.file_path === file);
  if (!row) throw new HistoryError('HISTORY_NOT_FOUND', 'This file was not part of the selected operation.');
  const status = row[`${input.side}_status`];
  const oid = row[`${input.side}_oid`];
  if (status !== 'captured' || !oid) return { ok: true, status, content: null, reason: row[`${input.side}_reason`], next: null };
  const bytes = await (await ctx.store()).readBlob(oid, 2 * 1024 * 1024);
  const end = Math.min(bytes.byteLength, input.offset + input.limit);
  const hasMore = end < bytes.byteLength;
  return { ok: true, status, oid, mode: row[`${input.side}_mode`], encoding: 'base64', content: Buffer.from(bytes.subarray(input.offset, end)).toString('base64'),
    total_bytes: bytes.byteLength, offset: input.offset, partial: hasMore,
    next: hasMore ? continuation(ctx, 'read', { ...input, file, offset: end }) : null };
}

export function historyInspect(ctx: HistoryContext, input: HistoryInspectInput) {
  const operation = historyOperation(ctx, input.operation_id);
  const scope = historyHash(JSON.stringify([ctx.dbPath, ctx.requestWorkspace ?? ctx.workspace, ctx.workspace, operation.operation_id]));
  const revision = historyHash(JSON.stringify(operation));
  let after = -1;
  if (input.cursor) {
    try {
      const cursor = JSON.parse(Buffer.from(input.cursor, 'base64url').toString('utf8')) as { scope: string; revision: string; after: number };
      if (cursor.scope !== scope || !Number.isSafeInteger(cursor.after) || cursor.after < 0) throw new Error();
      if (cursor.revision !== revision) return { ok: true, operation, rows: [], partial: true, partialReasons: ['snapshot_changed'],
        next: continuation(ctx, 'inspect', { operation_id: input.operation_id, limit: input.limit }) };
      after = cursor.after;
    } catch { throw new HistoryError('HISTORY_CURSOR_INVALID', 'Inspect cursor does not match this caller, source and operation.'); }
  }
  const versions = ctx.db.prepare('SELECT * FROM local_history_versions WHERE operation_id=? AND ordinal>? ORDER BY ordinal LIMIT ?')
    .all(input.operation_id, after, input.limit + 1).map(row => historyEntitySchemas.local_history_version.parse(row));
  const page = versions.slice(0, input.limit);
  const rows = page.map(row => ({ ...row, next: Object.fromEntries((['before', 'after'] as const)
    .filter(side => row[`${side}_status`] === 'captured' && row[`${side}_oid`])
    .map(side => [side, continuation(ctx, 'read', { operation_id: input.operation_id, file: row.file_path, side })])) }));
  const partial = versions.length > input.limit;
  return { ok: true, operation, source_workspace: ctx.workspace, rows, partial,
    next: partial ? continuation(ctx, 'inspect', { operation_id: input.operation_id, limit: input.limit,
      cursor: Buffer.from(JSON.stringify({ scope, revision, after: page.at(-1)!.ordinal })).toString('base64url') }) : null };
}
