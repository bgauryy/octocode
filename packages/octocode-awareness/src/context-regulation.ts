import { createHash } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { z } from 'zod';
import { appendDomainEvent } from './event-outbox.js';
import {
  interpretRunState,
  selectContextNudge,
  type ContextAdvisory,
  type ContextAdvisoryKind,
  type ContextNudge,
} from './context-state.js';
export type { ContextAdvisory, ContextAdvisoryKind } from './context-state.js';

const label = z.string().trim().min(1).max(200);
const opaqueId = z.string().min(1).max(128).regex(/^[A-Za-z0-9_.:-]+$/);
const timestamp = z.string().datetime({ offset: true });
export const contextObservationSchema = z.object({
  observation_id: opaqueId, observed_at: timestamp, action_fingerprint: label.optional(),
  source: z.enum(['agent', 'host']).default('agent'),
  acquisition: z.enum(['active', 'passive']).default('active').describe('Active inspection or a passive lifecycle observation; independent of who reported it.'),
  outcome: z.enum(['success', 'failure']).optional(), evidence_revision: label.optional(),
  progress: z.object({ scope: label, value: z.number().finite().nonnegative() }).strict().optional(),
  context: z.object({ used: z.number().finite().nonnegative(), limit: z.number().finite().positive() }).strict().optional(),
}).strict().superRefine((value, ctx) => {
  if (Boolean(value.action_fingerprint) !== Boolean(value.outcome)) ctx.addIssue({ code: 'custom',
    message: 'action_fingerprint and outcome must be supplied together', path: ['action_fingerprint'] });
  if (!value.action_fingerprint && !value.progress && !value.context) ctx.addIssue({ code: 'custom',
    message: 'at least one action, progress or context sensor is required' });
});
export const contextFeedbackSchema = z.object({
  feedback_id: opaqueId, observed_at: timestamp, advisory_id: opaqueId,
  observation_id: opaqueId.optional(),
  action_taken: z.string().trim().min(1).max(500), outcome: z.enum(['helpful', 'unnecessary', 'unresolved']),
}).strict().refine(value => value.outcome !== 'helpful' || Boolean(value.observation_id), {
  message: 'helpful feedback requires an observation_id for the reported result', path: ['observation_id'],
});
// Full object alternatives keep the published contract executable by consumers
// using Zod's JSON Schema reader, including the co-required sensor fields.
export const contextObservationJsonSchema = z.toJSONSchema(z.union([
  contextObservationSchema.safeExtend({ action_fingerprint: label, outcome: z.enum(['success', 'failure']) }),
  contextObservationSchema.safeExtend({ action_fingerprint: z.never().optional(), outcome: z.never().optional(),
    context: contextObservationSchema.shape.context.unwrap() }),
  contextObservationSchema.safeExtend({ action_fingerprint: z.never().optional(), outcome: z.never().optional(),
    progress: contextObservationSchema.shape.progress.unwrap() }),
]), { io: 'input' });
export const contextFeedbackJsonSchema = z.toJSONSchema(z.union([
  contextFeedbackSchema.safeExtend({ outcome: z.literal('helpful'), observation_id: contextFeedbackSchema.shape.advisory_id }),
  contextFeedbackSchema.safeExtend({ outcome: z.enum(['unnecessary', 'unresolved']) }),
]), { io: 'input' });
export type ContextObservation = z.input<typeof contextObservationSchema>;
export type ContextFeedback = z.infer<typeof contextFeedbackSchema>;
export interface ContextRegulationScope { workspace: string; actorId: string; sessionId: string }
interface State { repetition: number; stalled: number; recent_ids: string[]; roots: Partial<Record<ContextAdvisoryKind, string>> }
interface ObservationRecord { report: ContextObservation; state: State; advisories: ContextAdvisory[]; sequence?: number; progressing?: boolean; passive_nudge?: ContextNudge }
const WINDOW = 64;
export const CONTEXT_OBSERVATION_MAX_AGE_MS = 300_000;
const digest = (value: unknown) => createHash('sha256').update(JSON.stringify(value)).digest('hex');
const handledEventId = (scope: ContextRegulationScope, advisoryId: string) =>
  `ctx_${digest([aggregate(scope), 'context.advisory-handled', advisoryId])}`;
function rejectFuture(at: string, now = new Date().toISOString()) {
  const clock = Date.parse(now);
  if (!Number.isFinite(clock)) throw new Error('now must be an ISO timestamp');
  if (Date.parse(at) > clock + 30_000) throw new Error('reported timestamp is more than 30 seconds in the future');
}
function aggregate(scope: ContextRegulationScope): string {
  for (const [key, value] of Object.entries(scope)) if (!value.trim()) throw new Error(`${key} is required`);
  return digest([scope.workspace, scope.actorId, scope.sessionId]);
}
function rows(db: DatabaseSync, scope: ContextRegulationScope): ObservationRecord[] {
  return (db.prepare(`SELECT payload_json, sequence FROM event_outbox WHERE workspace_path = ?
    AND aggregate_kind = 'context-regulation' AND aggregate_id = ? AND event_type = 'context.observed'
    ORDER BY sequence DESC LIMIT ?`).all(scope.workspace, aggregate(scope), WINDOW + 1) as Array<{ payload_json: string; sequence: number }>)
    .map(row => ({ ...JSON.parse(row.payload_json) as ObservationRecord, sequence: Number(row.sequence) }));
}
function append(db: DatabaseSync, scope: ContextRegulationScope, id: string, at: string, eventType: string, payload: unknown) {
  return appendDomainEvent(db, { workspace: scope.workspace, actorId: scope.actorId, sessionId: scope.sessionId,
    aggregateKind: 'context-regulation', aggregateId: aggregate(scope), eventType, retentionClass: 'operational',
    eventId: `ctx_${digest([aggregate(scope), eventType, id])}`, createdAt: at, payload });
}
const guidance: Record<ContextAdvisoryKind, [string, string]> = {
  repetition: ['At least three equivalent attempts reported unchanged evidence and progress.', 'Change the approach or collect new evidence before repeating.'],
  'stalled-progress': ['At least three observations reported no progress in the same metric scope.', 'Inspect the blocker and choose a verifiable next milestone.'],
  'context-pressure': ['Reported context usage is at least 90% of capacity.', 'Checkpoint the objective, evidence and next action; let the host compact context.'],
  'tool-failure': ['The latest action reported failure.', 'Inspect the failure evidence before retrying.'],
};
function derive(scope: ContextRegulationScope, report: ContextObservation, prior?: ObservationRecord): ObservationRecord {
  const previous = prior?.report;
  const sameProgress = Boolean(previous?.progress && report.progress && previous.evidence_revision && report.evidence_revision
    && previous.progress.scope === report.progress.scope
    && previous.progress.value === report.progress.value && previous.evidence_revision === report.evidence_revision);
  const sameAttempt = sameProgress && Boolean(previous?.action_fingerprint && report.action_fingerprint)
    && previous?.action_fingerprint === report.action_fingerprint;
  const state: State = { repetition: sameAttempt ? Math.min(WINDOW, (prior?.state.repetition ?? 0) + 1) : 1,
    stalled: sameProgress ? Math.min(WINDOW, (prior?.state.stalled ?? 0) + 1) : 1,
    recent_ids: [...(prior?.state.recent_ids ?? []), report.observation_id].slice(-3), roots: {} };
  const active: Record<ContextAdvisoryKind, boolean> = {
    repetition: Boolean(report.progress) && state.repetition >= 3,
    'stalled-progress': Boolean(report.progress) && state.stalled >= 3,
    'context-pressure': Boolean(report.context && report.context.used / report.context.limit >= 0.9),
    'tool-failure': report.outcome === 'failure',
  };
  const advisories: ContextAdvisory[] = [];
  for (const kind of Object.keys(active) as ContextAdvisoryKind[]) {
    if (!active[kind]) {
      // An omitted sensor is not a recovery sample and must not re-arm a warning.
      if ((kind === 'context-pressure' && !report.context) || (kind === 'tool-failure' && !report.outcome)) {
        if (prior?.state.roots[kind]) state.roots[kind] = prior.state.roots[kind];
      }
      continue;
    }
    const root = prior?.state.roots[kind] ?? report.observation_id;
    state.roots[kind] = root;
    const observation_ids = kind === 'repetition' || kind === 'stalled-progress' ? state.recent_ids : [report.observation_id];
    advisories.push({ id: `adv_${digest([aggregate(scope), kind, root]).slice(0, 24)}`, kind, observation_ids,
      reason: guidance[kind][0], suggested_action: guidance[kind][1] });
  }
  const progressing = Boolean(previous?.progress && report.progress && previous.progress.scope === report.progress.scope
    && report.progress.value > previous.progress.value);
  const newAdvice = advisories.filter(a => prior?.state.roots[a.kind] !== state.roots[a.kind]);
  const passive_nudge = report.acquisition === 'passive' ? selectContextNudge(newAdvice) : undefined;
  return { report, state, advisories, progressing, ...(passive_nudge ? { passive_nudge } : {}) };
}

function observationView(record: ObservationRecord, now: string) {
  const age = Date.parse(now) - Date.parse(record.report.observed_at);
  const fresh = age >= 0 && age <= CONTEXT_OBSERVATION_MAX_AGE_MS;
  return { run_state: interpretRunState(fresh, record.advisories, record.progressing),
    ...(fresh && record.passive_nudge ? { nudge: record.passive_nudge,
      ...(record.advisories.length > 1 ? { next: { operation: 'context.orient' as const } } : {}),
    } : {}) };
}

/** Explicit attributed report. The caller owns the durable transaction. */
export function observeContext(db: DatabaseSync, scope: ContextRegulationScope, input: ContextObservation, options: { now?: string } = {}) {
  const report = contextObservationSchema.parse(input);
  const now = options.now ?? new Date().toISOString();
  rejectFuture(report.observed_at, now);
  const eventId = `ctx_${digest([aggregate(scope), 'context.observed', report.observation_id])}`;
  const duplicate = db.prepare('SELECT payload_json, sequence FROM event_outbox WHERE event_id = ?').get(eventId) as
    { payload_json: string; sequence: number } | undefined;
  if (duplicate) {
    const existing = JSON.parse(duplicate.payload_json) as ObservationRecord;
    if (JSON.stringify(existing.report) !== JSON.stringify(report)) throw new Error('observation_id already has a different report');
    return { observation_id: report.observation_id, sequence: Number(duplicate.sequence), replayed: true, ...observationView(existing, now) };
  }
  const prior = rows(db, scope)[0];
  const elapsed = prior ? Date.parse(report.observed_at) - Date.parse(prior.report.observed_at) : 0;
  if (elapsed < 0) throw new Error('observation timestamp precedes the latest observation');
  const record = derive(scope, report, elapsed <= CONTEXT_OBSERVATION_MAX_AGE_MS ? prior : undefined);
  return { observation_id: report.observation_id,
    sequence: append(db, scope, report.observation_id, report.observed_at, 'context.observed', record), replayed: false, ...observationView(record, now) };
}

/** Read-only current assessment; absence and stale sensors are unknown, never recovery failures. */
export function assessContextRegulation(db: DatabaseSync, scope: ContextRegulationScope, options: { now?: string } = {}) {
  const records = rows(db, scope);
  const latest = records[0];
  const now = Date.parse(options.now ?? new Date().toISOString());
  if (!Number.isFinite(now)) throw new Error('now must be an ISO timestamp');
  const age = latest ? now - Date.parse(latest.report.observed_at) : null;
  const fresh = age !== null && age >= 0 && age <= CONTEXT_OBSERVATION_MAX_AGE_MS;
  const feedback = db.prepare(`SELECT json_extract(payload_json, '$.advisory_id') AS advisory_id,
    json_extract(payload_json, '$.outcome') AS outcome FROM event_outbox WHERE workspace_path = ?
    AND aggregate_kind = 'context-regulation' AND aggregate_id = ? AND event_type = 'context.feedback'
    ORDER BY sequence DESC LIMIT ?`).all(scope.workspace, aggregate(scope), WINDOW + 1) as Array<{ advisory_id: string; outcome: ContextFeedback['outcome'] }>;
  const boundedFeedback = feedback.slice(0, WINDOW);
  // Exact per-advisory lookup keeps handled episodes suppressed after unrelated feedback rolls out of the metrics window.
  const handled = new Set((latest?.advisories ?? []).filter(advisory => db.prepare('SELECT 1 FROM event_outbox WHERE event_id = ?')
    .get(handledEventId(scope, advisory.id))).map(a => a.id));
  return { trust: 'attributed-data' as const,
    run_state: interpretRunState(fresh, latest?.advisories ?? [], latest?.progressing),
    ...(latest ? { observation_id: latest.report.observation_id, source: latest.report.source ?? 'agent', acquisition: latest.report.acquisition ?? 'active',
      ...(fresh && latest.report.context ? { context: latest.report.context } : {}),
      ...(fresh && latest.report.progress ? { progress: latest.report.progress } : {}),
    } : {}),
    observed_at: latest?.report.observed_at ?? null, freshness: fresh ? 'fresh' as const : latest ? 'stale' as const : 'unavailable' as const,
    unavailable: [!fresh || !latest?.report.context ? 'context' : null, !fresh || !latest?.report.progress ? 'progress' : null,
      !fresh || !latest?.report.outcome ? 'tool_health' : null,
      !fresh || !latest?.report.progress || !latest.report.evidence_revision ? 'stalled_progress' : null,
      !fresh || !latest?.report.progress || !latest.report.evidence_revision || !latest.report.action_fingerprint ? 'repetition' : null,
    ].filter((x): x is string => x !== null),
    advisories: fresh && latest ? latest.advisories.filter(a => !handled.has(a.id)) : [],
    usefulness: { basis: 'reported-feedback' as const, helpful: boundedFeedback.filter(f => f.outcome === 'helpful').length,
      unnecessary: boundedFeedback.filter(f => f.outcome === 'unnecessary').length, unresolved: boundedFeedback.filter(f => f.outcome === 'unresolved').length },
    coverage: { scope: 'workspace-actor-session' as const, observation_limit: WINDOW, observations: Math.min(records.length, WINDOW),
      feedback_limit: WINDOW, feedback: boundedFeedback.length,
      terminal_limit: records.length > WINDOW || feedback.length > WINDOW ? { kind: 'assessment-window' as const,
        reason: 'Only the latest 64 observations and 64 feedback reports are assessed; this is not global history.' } : null },
  };
}

/** Feedback describes a host-reported intervention outcome; it cannot verify work or tests. */
export function recordContextFeedback(db: DatabaseSync, scope: ContextRegulationScope, input: ContextFeedback, options: { now?: string } = {}) {
  const feedback = contextFeedbackSchema.parse(input);
  rejectFuture(feedback.observed_at, options.now);
  const eventId = `ctx_${digest([aggregate(scope), 'context.feedback', feedback.feedback_id])}`;
  const duplicate = db.prepare('SELECT payload_json FROM event_outbox WHERE event_id = ?').get(eventId) as { payload_json: string } | undefined;
  if (duplicate && JSON.stringify(JSON.parse(duplicate.payload_json)) !== JSON.stringify(feedback)) throw new Error('feedback_id already has a different report');
  if (!duplicate) {
    const records = rows(db, scope).slice(0, WINDOW);
    const origin = records.filter(r => r.advisories.some(a => a.id === feedback.advisory_id)).at(-1);
    if (!origin) throw new Error('unknown advisory in this workspace, actor, session and assessment window');
    if (Date.parse(feedback.observed_at) < Date.parse(origin.report.observed_at)) throw new Error('feedback precedes the advisory');
    if (feedback.observation_id) {
      const result = records.find(r => r.report.observation_id === feedback.observation_id);
      if (!result || (result.sequence ?? 0) <= (origin.sequence ?? 0)
        || Date.parse(feedback.observed_at) < Date.parse(result.report.observed_at)) {
        throw new Error('feedback observation must follow the advisory and precede the feedback in this scope');
      }
    }
  }
  const sequence = append(db, scope, feedback.feedback_id, feedback.observed_at, 'context.feedback', feedback);
  if (feedback.outcome !== 'unresolved' && !db.prepare('SELECT 1 FROM event_outbox WHERE event_id = ?')
    .get(handledEventId(scope, feedback.advisory_id))) {
    append(db, scope, feedback.advisory_id, feedback.observed_at, 'context.advisory-handled', {
      advisory_id: feedback.advisory_id, feedback_id: feedback.feedback_id, disposition: 'handled',
    });
  }
  return { feedback_id: feedback.feedback_id, sequence, replayed: Boolean(duplicate) };
}
