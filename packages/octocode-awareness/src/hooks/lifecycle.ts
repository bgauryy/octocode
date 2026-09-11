import { writeCommandDiagnostic } from '../command-output.js';
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
import { resolveDbPath } from '../db-runtime.js';
import { auditUnverified } from '../verify-audit.js';
import { digest } from '../maintenance-digest.js';
import { peerBriefing } from '../peer-briefing.js';
import { sessionCapture } from '../maintenance-session.js';
import { endSession } from '../sessions.js';
import { agentId, artifact, completeHookControl, db, hookBlockOutcome, hookContextEnvelope, hookEventName, hookReason, hookSessionCorrelation, isStopHookActive, sessionId, shellHookHost, workspace, writeHookPayload } from './payload.js';
import { leaveHookAgent, registerHookAgent, scopeArgs } from './peers.js';
import { finalizeActiveFallbackHookRuns, withHookDbRetry } from './run-state.js';
import { AwarenessFeatureConfig, DEFAULT_AWARENESS_CONFIG } from '../awareness-config.js';
import { verificationDebtSignal } from './signals.js';

export async function runStopVerify(
  payload: Record<string, unknown>,
  features: AwarenessFeatureConfig = DEFAULT_AWARENESS_CONFIG.features,
): Promise<number> {
  const database = db(payload);
  registerHookAgent(database, payload, 'hook:stop-verify');
  const finalizedRunIds = withHookDbRetry(() => finalizeActiveFallbackHookRuns(
    database,
    payload,
    workspace(payload) ?? process.cwd(),
  ));
  if (!features.verificationGate || process.env.OCTOCODE_NO_VERIFY_GATE === '1') return 0;
  const report = auditUnverified(database, { agentId: agentId(payload), ...scopeArgs(payload) });
  if (report.count > 0) {
    // A recursive Stop with no newly finalized work already surfaced this
    // unchanged debt. Allow it to conclude to avoid an infinite host loop.
    // New continuation edits create/finalize a new aggregate and must surface
    // one fresh continuation before the following unchanged recursive Stop.
    if (isStopHookActive(payload) && finalizedRunIds.length === 0) return 0;
    return completeHookControl(hookBlockOutcome(
      shellHookHost(payload),
      'stop',
      verificationDebtSignal(report.count),
    ));
  }
  return 0;
}

function digestPreviewSchedule(
  payload: Record<string, unknown>,
  features: AwarenessFeatureConfig,
): { markerPath: string; now: number; due: boolean } | null {
  if (!features.maintenanceReminders || process.env.OCTOCODE_NO_DIGEST === '1') return null;
  const intervalHours = Number(process.env.OCTOCODE_DIGEST_INTERVAL_HOURS ?? 4);
  const intervalMs = Number.isFinite(intervalHours) && intervalHours > 0 ? intervalHours * 3600_000 : 4 * 3600_000;
  const memoryHome = dirname(resolveDbPath(null));
  const digestScope = workspace(payload) ?? 'global';
  const scopeHash = createHash('sha256').update(digestScope).digest('hex').slice(0, 12);
  const markerPath = join(memoryHome, `.last-digest-preview-${scopeHash}-epoch-ms`);
  let last = 0;
  try { last = Number(readFileSync(markerPath, 'utf8').trim() || 0); } catch { /* first preview */ }
  const now = Date.now();
  return { markerPath, now, due: !last || now < last || now - last >= intervalMs };
}

/** A cheap filesystem deadline check; unchanged SQLite bytes do not stop time. */
export function isDigestPreviewDue(payload: Record<string, unknown>, features: AwarenessFeatureConfig): boolean {
  return digestPreviewSchedule(payload, features)?.due ?? false;
}

export function maybePreviewDigest(
  payload: Record<string, unknown>,
  features: AwarenessFeatureConfig = DEFAULT_AWARENESS_CONFIG.features,
): string | null {
  try {
    const schedule = digestPreviewSchedule(payload, features);
    if (schedule?.due) {
      const { markerPath, now } = schedule;
      const database = db(payload, 'digest');
      const preview = digest(database, {
        workspace_path: workspace(payload),
        dry_run: true,
      });
      mkdirSync(dirname(markerPath), { recursive: true });
      writeFileSync(markerPath, String(now), 'utf8');
      const pressure = {
        archive: preview.would_archive ?? 0,
        memories: preview.would_prune_old ?? 0,
        locks: preview.would_prune_locks ?? 0,
        refinements: preview.would_prune_refinements ?? 0,
      };
      if (Object.values(pressure).some((count) => count > 0)) {
        return `Maintenance pressure: archive ${pressure.archive}, prune memories ${pressure.memories}, locks ${pressure.locks}, refinements ${pressure.refinements}. Review with npx @octocodeai/octocode-awareness maintenance digest --dry-run --workspace "$PWD" --compact; apply only after review.`;
      }
    }
  } catch (error) {
    writeCommandDiagnostic(`octocode-awareness digest warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return null;
}

export async function runNotifyDeliver(
  payload: Record<string, unknown>,
  features: AwarenessFeatureConfig = DEFAULT_AWARENESS_CONFIG.features,
): Promise<number> {
  return runCommunication(payload, features);
}

export function canDeliverHookCommunication(payload: Record<string, unknown>): boolean {
  const host = shellHookHost(payload);
  const event = hookEventName(payload) ?? (host === 'cursor' ? 'sessionStart' : 'UserPromptSubmit');
  return Object.keys(hookContextEnvelope(host, event, 'Awareness peer message')).length > 0;
}

/** Read and offer relevant coordination context without settling work or acknowledging delivery. */
export async function runToolCommunication(
  payload: Record<string, unknown>,
  features: AwarenessFeatureConfig = DEFAULT_AWARENESS_CONFIG.features,
): Promise<number> {
  return runCommunication(payload, features);
}

async function runCommunication(
  payload: Record<string, unknown>,
  features: AwarenessFeatureConfig,
): Promise<number> {
  if (process.env.OCTOCODE_NO_NOTIFY === '1' || !canDeliverHookCommunication(payload)) return 0;
  const maintenanceContext = maybePreviewDigest(payload, features);
  const database = db(payload);
  registerHookAgent(database, payload, 'hook:notify-deliver');
  const messageContext = features.notifications
    ? peerBriefing(database, {
        agentId: agentId(payload), sessionId: hookSessionCorrelation(payload) ?? undefined,
        workspacePath: workspace(payload) ?? process.cwd(), artifact: artifact(payload),
      })
    : null;
  const context = [messageContext, maintenanceContext].filter(Boolean).join('\n');
  if (context) {
    const outputEvent = hookEventName(payload) ?? (shellHookHost(payload) === 'cursor' ? 'sessionStart' : 'UserPromptSubmit');
    const envelope = hookContextEnvelope(
      shellHookHost(payload), outputEvent,
      context,
    );
    // Some native events, such as Cursor postToolUseFailure, expose no context channel.
    if (Object.keys(envelope).length > 0) writeHookPayload(envelope);
  }
  return 0;
}

export async function runSessionEnd(
  payload: Record<string, unknown>,
  features: AwarenessFeatureConfig = DEFAULT_AWARENESS_CONFIG.features,
  options: { settleWork?: boolean } = {},
): Promise<number> {
  const database = db(payload);
  registerHookAgent(database, payload, 'hook:session-end');
  if (options.settleWork !== false) withHookDbRetry(() => finalizeActiveFallbackHookRuns(
    database,
    payload,
    workspace(payload) ?? process.cwd(),
  ));
  if (options.settleWork !== false && features.sessionCapture && process.env.OCTOCODE_NO_SESSION_CAPTURE !== '1' && hookReason(payload) !== 'clear') {
    sessionCapture(database, {
      agent_id: agentId(payload),
      workspace_path: workspace(payload) ?? undefined,
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
  leaveHookAgent(database, payload);
  return 0;
}

export async function runSessionCompact(
  payload: Record<string, unknown>,
  features: AwarenessFeatureConfig = DEFAULT_AWARENESS_CONFIG.features,
): Promise<number> {
  const database = db(payload);
  registerHookAgent(database, payload, 'hook:session-compact');
  withHookDbRetry(() => finalizeActiveFallbackHookRuns(
    database,
    payload,
    workspace(payload) ?? process.cwd(),
  ));
  if (features.sessionCapture && process.env.OCTOCODE_NO_SESSION_CAPTURE !== '1' && hookReason(payload) !== 'clear') {
    sessionCapture(database, {
      agent_id: agentId(payload),
      workspace_path: workspace(payload) ?? undefined,
      artifact: artifact(payload) ?? undefined,
      reason: hookReason(payload) || 'compact',
    });
  }
  // PreCompact is a turn boundary, not a session boundary. Keep the session
  // reusable so the host can continue with the same correlation id.
  return 0;
}
