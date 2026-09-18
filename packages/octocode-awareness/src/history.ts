import type { DatabaseSync } from './sqlite.js';
import { HISTORY_ROUTE_DESCRIPTORS, historyRequestSchemas } from './schema/definitions-history.js';
import { captureHistory } from './history-capture.js';
import { historyEvidence, historyInspect, historyRead, historyRecovery, historyRetentionPreview, historyRetentionPrune, historyStatus, historyTimeline } from './history-query.js';
import { canonicalizePath, repositoryWorkspacePaths, withRepositoryWorkspaceScope } from './git.js';
import { applyHistoryRestore, previewHistoryRestore } from './history-restore.js';
import { createHistoryContext, HistoryError } from './history-store.js';

/** Async history boundary shared by CLI and native lifecycle adapters. Model input never carries runtime handles. */
export async function runAwarenessHistoryOperation(db: DatabaseSync, command: string, request: unknown): Promise<Record<string, unknown>> {
  const route = HISTORY_ROUTE_DESCRIPTORS.find(entry => entry.command === `history ${command}`);
  if (!route) throw new HistoryError('HISTORY_INVALID_REQUEST', 'Unknown history command. Use schema commands for supported routes.');
  const parsed = historyRequestSchemas[route.schema].safeParse(request);
  if (!parsed.success) throw new HistoryError('HISTORY_INVALID_REQUEST', parsed.error.issues.map(issue => `${issue.path.join('.')}: ${issue.message}`).join('; '));
  const ctx = createHistoryContext(db, parsed.data.workspace);
  if (route.schema === 'history_read' || route.schema === 'history_inspect') {
    const input = parsed.data as { source_workspace?: string };
    const source = input.source_workspace ? canonicalizePath(input.source_workspace) : ctx.workspace;
    const readSource = async () => {
      if (source !== ctx.workspace && !repositoryWorkspacePaths(ctx.workspace).includes(source)) {
        throw new HistoryError('HISTORY_SOURCE_WORKSPACE', 'History source must be the caller workspace or a live linked worktree in the same repository.');
      }
      const sourceContext = { ...createHistoryContext(db, source, { readOnly: true }),
        ...(source !== ctx.workspace ? { requestWorkspace: ctx.workspace } : {}) };
      return route.schema === 'history_inspect'
        ? historyInspect(sourceContext, historyRequestSchemas.history_inspect.parse(parsed.data))
        : historyRead(sourceContext, historyRequestSchemas.history_read.parse(parsed.data));
    };
    return source === ctx.workspace ? readSource() : withRepositoryWorkspaceScope(ctx.workspace, readSource);
  }
  switch (route.schema) {
    case 'history_status': return historyStatus(ctx);
    case 'history_capture': return captureHistory(ctx, historyRequestSchemas.history_capture.parse(parsed.data));
    case 'history_checkpoint': return captureHistory(ctx, historyRequestSchemas.history_checkpoint.parse(parsed.data));
    case 'history_timeline': return historyTimeline(ctx, historyRequestSchemas.history_timeline.parse(parsed.data));
    case 'history_restore_preview': return previewHistoryRestore(ctx, historyRequestSchemas.history_restore_preview.parse(parsed.data));
    case 'history_restore_apply': return applyHistoryRestore(ctx, historyRequestSchemas.history_restore_apply.parse(parsed.data));
    case 'history_retention_preview': return historyRetentionPreview(ctx, historyRequestSchemas.history_retention_preview.parse(parsed.data));
    case 'history_retention_prune': return historyRetentionPrune(ctx, historyRequestSchemas.history_retention_prune.parse(parsed.data));
    case 'history_recovery': return historyRecovery(ctx, historyRequestSchemas.history_recovery.parse(parsed.data));
    case 'history_evidence': return historyEvidence(ctx, historyRequestSchemas.history_evidence.parse(parsed.data));
  }
}
