/**
 * hook-runner.ts — shared implementation for octocode-awareness lifecycle hooks.
 *
 * Shell hook files are intentionally thin wrappers. All parsing, file presence,
 * verification, briefing, and session-capture logic lives here so Claude/Codex
 * skill hooks and Pi native adapters share the same package-owned behavior.
 */
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { resolveDbPath } from '../src/db.js';
import { auditUnverified } from '../src/verify.js';
import { digest, notifyGet, sessionCapture } from '../src/maintenance.js';
import { endSession } from '../src/sessions.js';
import { agentId, artifact, completeHookControl, db, emitHookContext, hookBlockOutcome, hookEventName, hookReason, hookSessionCorrelation, isStopHookActive, promptQuery, sessionId, shellHookHost, workspace } from './hook-payload.js';
import { registerHookAgent, scopeArgs } from './hook-peers.js';
import { finalizeActiveFallbackHookRuns, withHookDbRetry } from './hook-run-state.js';

export async function runStopVerify(payload: Record<string, unknown>): Promise<number> {
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:stop-verify');
    const finalizedRunIds = withHookDbRetry(() => finalizeActiveFallbackHookRuns(
      database,
      payload,
      workspace(payload) ?? process.cwd(),
    ));
    if (process.env.OCTOCODE_NO_VERIFY_GATE === '1') return 0;
    const report = auditUnverified(database, { agentId: agentId(payload), ...scopeArgs(payload) });
    if (report.count > 0) {
      // A recursive Stop with no newly finalized work already surfaced this
      // unchanged debt. Allow it to conclude to avoid an infinite host loop.
      // New continuation edits create/finalize a new aggregate and must surface
      // one fresh continuation before the following unchanged recursive Stop.
      if (isStopHookActive(payload) && finalizedRunIds.length === 0) return 0;
      const details = [
        ...report.unverified.map((run) => `${run.status}:${run.run_id}: ${run.test_plan}`),
        ...report.stale_active.map((run) => `STALE:${run.run_id}: ${run.rationale}`),
      ];
      const shown = details.slice(0, 3);
      const omitted = details.length > 3 ? `; +${details.length - 3} omitted` : '';
      return completeHookControl(hookBlockOutcome(
        shellHookHost(payload),
        'stop',
        `octocode-awareness: concluding with unverified work. ${shown.join('; ')}${omitted}`,
      ));
    }
  } catch (error) {
    console.error(`octocode-awareness verify warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return 0;
}

export function maybePreviewDigest(payload: Record<string, unknown>): string | null {
  if (process.env.OCTOCODE_NO_DIGEST === '1') return null;
  if (process.env.OCTOCODE_NOTIFY_RUN_DIGEST !== '1') return null;
  const intervalHours = Number(process.env.OCTOCODE_DIGEST_INTERVAL_HOURS ?? 4);
  const intervalMs = Number.isFinite(intervalHours) && intervalHours > 0 ? intervalHours * 3600_000 : 4 * 3600_000;
  const memoryHome = dirname(resolveDbPath(null));
  const digestScope = workspace(payload) ?? 'global';
  const scopeHash = createHash('sha256').update(digestScope).digest('hex').slice(0, 12);
  const markerPath = join(memoryHome, `.last-digest-preview-${scopeHash}-epoch-ms`);
  try {
    const database = db();
    let last = 0;
    try {
      last = Number(readFileSync(markerPath, 'utf8').trim() || 0);
    } catch {
      last = 0;
    }
    const now = Date.now();
    if (!last || now - last >= intervalMs) {
      const preview = digest(database, {
        workspace: workspace(payload),
        memoryHome,
        dry_run: true,
      });
      mkdirSync(memoryHome, { recursive: true });
      writeFileSync(markerPath, String(now), 'utf8');
      const pressure = {
        archive: preview.would_archive ?? 0,
        memories: preview.would_prune_old ?? 0,
        locks: preview.would_prune_locks ?? 0,
        refinements: preview.would_prune_refinements ?? 0,
      };
      if (Object.values(pressure).some((count) => count > 0)) {
        return `Maintenance pressure: archive ${pressure.archive}, prune memories ${pressure.memories}, locks ${pressure.locks}, refinements ${pressure.refinements}. Review with octocode-awareness maintenance digest --dry-run --workspace "$PWD" --compact; apply only after review.`;
      }
    }
  } catch (error) {
    console.error(`octocode-awareness digest warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return null;
}

export async function runNotifyDeliver(payload: Record<string, unknown>): Promise<number> {
  if (process.env.OCTOCODE_NO_NOTIFY === '1') return 0;
  const maintenanceContext = maybePreviewDigest(payload);
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:notify-deliver');
    withHookDbRetry(() => finalizeActiveFallbackHookRuns(
      database,
      payload,
      workspace(payload) ?? process.cwd(),
    ));
    const result = notifyGet(database, {
      agent_id: agentId(payload),
      session_id: hookSessionCorrelation(payload) ?? undefined,
      workspace: workspace(payload) ?? undefined,
      artifact: artifact(payload) ?? undefined,
      query: promptQuery(payload) ?? undefined,
      format: 'hook',
    }) as { additionalContext?: string };
    const additionalContext = [result.additionalContext, maintenanceContext].filter(Boolean).join('\n');
    if (additionalContext) {
      emitHookContext(
        payload,
        shellHookHost(payload) === 'cursor'
          ? hookEventName(payload) === 'subagentStart' ? 'subagentStart' : 'sessionStart'
          : hookEventName(payload) === 'SubagentStart'
            ? 'SubagentStart'
            : hookEventName(payload) === 'SessionStart' ? 'SessionStart' : 'UserPromptSubmit',
        additionalContext,
      );
    }
  } catch (error) {
    console.error(`octocode-awareness session-capture warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return 0;
}

export async function runSessionEnd(payload: Record<string, unknown>): Promise<number> {
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:session-end');
    withHookDbRetry(() => finalizeActiveFallbackHookRuns(
      database,
      payload,
      workspace(payload) ?? process.cwd(),
    ));
    if (process.env.OCTOCODE_NO_SESSION_CAPTURE !== '1' && hookReason(payload) !== 'clear') {
      sessionCapture(database, {
        agent_id: agentId(payload),
        workspace: workspace(payload) ?? undefined,
        artifact: artifact(payload) ?? undefined,
        reason: hookReason(payload) || undefined,
      });
    }
    // Mark the session ended so its still-held locks read as abandoned
    // (holder_session_active:false) to any agent that later conflicts on them.
    const sid = sessionId(payload);
    if (sid) endSession(database, {
      sessionId: sid,
      agentId: agentId(payload),
      workspacePath: workspace(payload) ?? process.cwd(),
      artifact: artifact(payload),
    });
  } catch {
    // fail-open
  }
  return 0;
}

export async function runSessionCompact(payload: Record<string, unknown>): Promise<number> {
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:session-compact');
    withHookDbRetry(() => finalizeActiveFallbackHookRuns(
      database,
      payload,
      workspace(payload) ?? process.cwd(),
    ));
    if (process.env.OCTOCODE_NO_SESSION_CAPTURE !== '1' && hookReason(payload) !== 'clear') {
      sessionCapture(database, {
        agent_id: agentId(payload),
        workspace: workspace(payload) ?? undefined,
        artifact: artifact(payload) ?? undefined,
        reason: hookReason(payload) || 'compact',
      });
    }
    // PreCompact is a turn boundary, not a session boundary. Keep the session
    // reusable so the host can continue with the same correlation id.
  } catch {
    // fail-open
  }
  return 0;
}
