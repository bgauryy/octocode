/** Hook-only rollback for structured writes that failed before changing a file. */

import type { DatabaseSync } from 'node:sqlite';
import { getRun, normalizeFiles } from './work.js';

export function discardUncommittedHookFiles(db: DatabaseSync, params: {
  agentId: string;
  runId: string;
  targetFiles: string[];
  workspacePath: string;
}): { discarded: number; deletedRun: boolean } {
  const run = getRun(db, params.runId);
  if (run.agent_id !== params.agentId) throw new Error(`run ${params.runId} belongs to ${run.agent_id}`);
  if (run.origin !== 'HOOK') return { discarded: 0, deletedRun: false };
  const targets = normalizeFiles(params.targetFiles, params.workspacePath);

  db.exec('BEGIN IMMEDIATE');
  try {
    const placeholders = targets.map(() => '?').join(',');
    db.prepare(`DELETE FROM locks WHERE run_id = ? AND file_path IN (${placeholders})`)
      .run(params.runId, ...targets);
    const discarded = db.prepare(`DELETE FROM run_files
      WHERE run_id = ? AND file_path IN (${placeholders})
        AND NOT EXISTS (
          SELECT 1 FROM edit_log
          WHERE edit_log.run_id = run_files.run_id AND edit_log.file_path = run_files.file_path
        )`).run(params.runId, ...targets) as { changes: number };
    const remaining = db.prepare('SELECT 1 FROM run_files WHERE run_id = ? LIMIT 1').get(params.runId);
    if (!remaining) db.prepare("DELETE FROM task_runs WHERE run_id = ? AND origin = 'HOOK'").run(params.runId);
    db.exec('COMMIT');
    return { discarded: discarded.changes, deletedRun: !remaining };
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* transaction did not open */ }
    throw error;
  }
}
