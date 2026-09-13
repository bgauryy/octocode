import type { DatabaseSync } from 'node:sqlite';
import { beginWrite } from './db-transaction.js';
import { pruneRetainedEvents } from './event-retention.js';
import { maintainExpiredHistoryPreviews } from './history-retention.js';
import { maintainInteractions } from './interaction-retention.js';
import { maintainMemories } from './memory-retention.js';
import { pruneExpiredNotifications } from './message-lifecycle.js';
import { pruneExpiredLocks } from './lock-retention.js';
import { pruneTerminalRuns, recoverStaleActiveRuns } from './run-retention.js';
import {
  MAINTENANCE_RETENTION_OPERATION,
  type MaintenanceRetentionInput,
} from './schema/definitions-maintenance.js';

const DAY_MS = 86_400_000;

function before(now: string, days: number): string {
  return new Date(Date.parse(now) - days * DAY_MS).toISOString();
}

/** Compose canonical lifecycle owners in one SQLite transaction for apply. */
export function runMaintenanceRetention(
  db: DatabaseSync,
  workspacePath: string,
  input: MaintenanceRetentionInput,
  options: { now?: string; dbPath?: string } = {},
): Record<string, unknown> {
  const now = input.as_of ?? options.now ?? new Date().toISOString();
  const dryRun = input.action === 'report';
  const terminalBefore = before(now, input.retention_days);
  const operationalBefore = before(now, input.operational_retention_days);
  const staleRunBefore = before(now, input.stale_run_age_days);

  const maintain = () => {
    const messages = pruneExpiredNotifications(db, {
      now,
      dryRun,
      workspacePath,
      pruneBefore: terminalBefore,
      limit: input.limit,
    });
    const events = pruneRetainedEvents(db, {
      workspacePath,
      now,
      deliveryBefore: terminalBefore,
      operationalBefore,
      limit: input.limit,
      dryRun,
    });
    const interactions = maintainInteractions(db, { workspacePath, terminalBefore, now, limit: input.limit, dryRun });
    const locks = pruneExpiredLocks(db, { workspacePath, now, limit: input.limit, dryRun });
    const staleRuns = recoverStaleActiveRuns(db, {
      workspacePath,
      before: staleRunBefore,
      now,
      limit: input.limit,
      apply: input.fail_stale_active_runs,
      dryRun,
    });
    const runs = pruneTerminalRuns(db, { workspacePath, before: terminalBefore, limit: input.limit, dryRun });
    const memories = maintainMemories(db, { workspacePath, terminalBefore, now, limit: input.limit, dryRun });
    const history = maintainExpiredHistoryPreviews(db, {
      workspacePath,
      now,
      limit: input.limit,
      dryRun,
    });
    const messageMatched = dryRun ? messages.wouldDelete ?? 0 : messages.deleted;
    const lockMatched = locks.matched;
    const matched = messageMatched + events.matched + interactions.matched + lockMatched + staleRuns.matched
      + runs.matched + memories.archived + memories.matched + history.matched;
    const deleteMatched = messageMatched + events.matched + interactions.matched + lockMatched
      + runs.matched + memories.matched + history.matched;
    const deleted = messages.deleted + events.deleted + interactions.deleted + locks.deleted
      + runs.deleted + memories.deleted + history.deleted;
    const partial = !dryRun && [messages, events, interactions, locks, staleRuns, runs, memories, history]
      .some(result => result.partial === true);
    const continuationArgs = {
      workspace: workspacePath,
      action: 'apply' as const,
      confirm: 'apply-retention' as const,
      retention_days: input.retention_days,
      operational_retention_days: input.operational_retention_days,
      stale_run_age_days: input.stale_run_age_days,
      fail_stale_active_runs: input.fail_stale_active_runs,
      limit: input.limit,
      as_of: now,
    };
    const next = partial ? {
      command: 'maintenance retention',
      args: continuationArgs,
      ...(options.dbPath ? {
        db: options.dbPath,
        argv: [
          'maintenance', 'retention', '--db', options.dbPath,
          '--workspace', workspacePath,
          '--action', 'apply', '--confirm', 'apply-retention',
          '--retention-days', String(input.retention_days),
          '--operational-retention-days', String(input.operational_retention_days),
          '--stale-run-age-days', String(input.stale_run_age_days),
          ...(input.fail_stale_active_runs ? ['--fail-stale-active-runs'] : []),
          '--limit', String(input.limit), '--as-of', now, '--compact',
        ],
      } : {}),
    } : null;
    return {
      ok: true,
      operation: MAINTENANCE_RETENTION_OPERATION,
      action: input.action,
      dry_run: dryRun,
      workspace: workspacePath,
      as_of: now,
      cutoffs: { terminal_before: terminalBefore, operational_before: operationalBefore, stale_run_before: staleRunBefore },
      domains: {
        messages: {
          matched: messageMatched,
          transitioned: messages.transitioned,
          deleted: messages.deleted,
          ...(dryRun ? {
            would_transition: messages.wouldTransition ?? 0,
            would_delete: messages.wouldDelete ?? 0,
          } : {}),
        },
        events,
        interactions,
        locks,
        stale_runs: staleRuns,
        runs,
        memories,
        history: { matched: history.matched, deleted: history.deleted, partial: history.partial },
      },
      totals: {
        matched,
        deleted,
        archived: memories.archived,
        expired: interactions.expired,
        transitioned: messages.transitioned,
        failed: staleRuns.failed,
        ...(dryRun ? {
          would_delete: deleteMatched,
          would_archive: memories.archived,
          would_expire: interactions.expired,
          would_transition: messages.wouldTransition ?? 0,
        } : {}),
      },
      partial,
      next,
    };
  };

  if (dryRun) return maintain();
  const transaction = beginWrite(db);
  try {
    const result = maintain();
    transaction.commit();
    return result;
  } catch (error) {
    transaction.rollback();
    throw error;
  }
}
