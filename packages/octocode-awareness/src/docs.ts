/**
 * docs.ts — Documentation staleness detection.
 *
 * Compares edit_log activity in a doc's source directories against the doc's
 * own last recorded edit, to flag docs (e.g. ARCHITECTURE.md) that have
 * drifted from the code they describe — an "AutoWiki"-style freshness signal
 * built entirely on data the harness already collects (edit_log), with no
 * new tables and no generation pipeline.
 *
 * mineDocStaleness():  pure read — never mutates the store.
 * proposeDocRefresh(): records a harness_log 'propose' event for one stale entry.
 */

import type { DatabaseSync } from 'node:sqlite';
import { insertHarnessLog } from './audit.js';
import { normalizeArtifact } from './helpers.js';
import type {
  DocStalenessParams,
  DocStalenessResult,
  DocStalenessEntry,
  ProposeDocRefreshParams,
} from './types.js';

const DEFAULT_MIN_EDITS_SINCE_SYNC = 5;
const DEFAULT_MIN_LINES_SINCE_SYNC = 50;

/** Most recent edit_log timestamp recorded for this exact file path, or null if never tracked. */
function lastEditTimestamp(db: DatabaseSync, filePath: string, workspacePath: string | null, artifact: string | null): string | null {
  const conditions = ['file_path = ?'];
  const binds: string[] = [filePath];
  if (workspacePath) {
    conditions.push('(workspace_path = ? OR workspace_path IS NULL)');
    binds.push(workspacePath);
  }
  if (artifact) {
    conditions.push('(artifact = ? OR artifact IS NULL)');
    binds.push(artifact);
  }
  const row = db.prepare(
    `SELECT MAX(created_at) AS ts FROM edit_log WHERE ${conditions.join(' AND ')}`
  ).get(...binds) as { ts: string | null } | undefined;
  return row?.ts ?? null;
}

interface SourceActivity {
  edits: number;
  linesChanged: number;
  files: string[];
  latest: string | null;
}

/**
 * Edits under any of sourceDirs strictly after `since` (or all-time when since is null —
 * a doc with no edit_log history has no known sync point, so all recorded activity counts).
 */
function sourceActivitySince(
  db: DatabaseSync,
  sourceDirs: string[],
  since: string | null,
  workspacePath: string | null,
  artifact: string | null,
): SourceActivity {
  if (sourceDirs.length === 0) return { edits: 0, linesChanged: 0, files: [], latest: null };

  const conditions: string[] = [];
  const binds: string[] = [];

  const dirClauses = sourceDirs.map(() => 'file_path LIKE ?');
  conditions.push(`(${dirClauses.join(' OR ')})`);
  binds.push(...sourceDirs.map((d) => `${d.replace(/\/+$/, '')}/%`));

  if (since) {
    conditions.push('created_at > ?');
    binds.push(since);
  }
  if (workspacePath) {
    conditions.push('(workspace_path = ? OR workspace_path IS NULL)');
    binds.push(workspacePath);
  }
  if (artifact) {
    conditions.push('(artifact = ? OR artifact IS NULL)');
    binds.push(artifact);
  }

  type Row = { file_path: string; lines_added: number | null; lines_removed: number | null; created_at: string };
  const rows = db.prepare(
    `SELECT file_path, lines_added, lines_removed, created_at
     FROM edit_log WHERE ${conditions.join(' AND ')}`
  ).all(...binds) as unknown as Row[];

  const files = [...new Set(rows.map((r) => r.file_path))];
  const linesChanged = rows.reduce((sum, r) => sum + (r.lines_added ?? 0) + (r.lines_removed ?? 0), 0);
  const latest = rows.reduce<string | null>(
    (max, r) => (!max || r.created_at > max ? r.created_at : max),
    null,
  );

  return { edits: rows.length, linesChanged, files, latest };
}

/**
 * Checks each configured doc-to-source mapping for drift and returns a staleness
 * report. Pure read — the caller decides whether to act on `stale` entries via
 * proposeDocRefresh(). Threshold defaults intentionally err toward under-flagging:
 * a handful of edits is normal churn, not drift.
 */
export function mineDocStaleness(db: DatabaseSync, params: DocStalenessParams): DocStalenessResult {
  const minEdits = params.minEditsSinceSync ?? DEFAULT_MIN_EDITS_SINCE_SYNC;
  const minLines = params.minLinesSinceSync ?? DEFAULT_MIN_LINES_SINCE_SYNC;
  const workspacePath = params.workspacePath ?? null;
  const artifact = normalizeArtifact(params.artifact);

  const entries: DocStalenessEntry[] = params.targets.map((target): DocStalenessEntry => {
    const docLastSyncedAt = lastEditTimestamp(db, target.docFile, workspacePath, artifact);
    const activity = sourceActivitySince(db, target.sourceDirs, docLastSyncedAt, workspacePath, artifact);
    const stale = activity.edits >= minEdits || activity.linesChanged >= minLines;

    return {
      doc_file: target.docFile,
      source_dirs: target.sourceDirs,
      doc_last_synced_at: docLastSyncedAt,
      edits_since_sync: activity.edits,
      lines_changed_since_sync: activity.linesChanged,
      files_touched: activity.files,
      latest_source_edit_at: activity.latest,
      stale,
    };
  });

  return {
    ok: true,
    checked: entries.length,
    stale_count: entries.filter((e) => e.stale).length,
    entries,
  };
}

/**
 * Records a harness_log 'propose' event for one stale entry — the same event
 * type failure-mined proposals use (see mineWeakness), so exportHarness and
 * harness-log queries surface doc-staleness proposals alongside skill fixes
 * without new plumbing. Returns the harness_id.
 */
export function proposeDocRefresh(
  db: DatabaseSync,
  entry: DocStalenessEntry,
  params: ProposeDocRefreshParams,
): string {
  const sinceLabel = entry.doc_last_synced_at ?? 'doc was last tracked (no prior edit_log record)';
  return insertHarnessLog(db, {
    agentId: params.agentId,
    sessionId: params.sessionId ?? null,
    workspacePath: params.workspacePath ?? null,
    artifact: params.artifact ?? null,
    eventType: 'propose',
    payload: {
      failure_signature: 'doc-staleness',
      target_file: entry.doc_file,
      proposed_change:
        `Refresh ${entry.doc_file} — ${entry.edits_since_sync} edit(s) / ` +
        `${entry.lines_changed_since_sync} line(s) changed across ${entry.source_dirs.join(', ')} ` +
        `since ${sinceLabel}.`,
      evidence: {
        edits_since_sync: entry.edits_since_sync,
        lines_changed_since_sync: entry.lines_changed_since_sync,
        files_touched: entry.files_touched,
        doc_last_synced_at: entry.doc_last_synced_at,
        latest_source_edit_at: entry.latest_source_edit_at,
      },
    },
  });
}
