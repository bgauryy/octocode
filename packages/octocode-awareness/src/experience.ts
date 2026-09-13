import type { DatabaseSync } from 'node:sqlite';
import { resolve } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { workspaceEventHighWater } from './event-outbox.js';
import { containsSecretLikeText } from './memory-hardening.js';
import { normalizeAnchors } from './knowledge-anchor.js';
import { archiveExperience, readExperienceArchive, type ExperienceArchiveReceipt } from './experience-archive.js';
import { experienceInputSchema, MAX_EXPERIENCE_EVENTS, MAX_EXPERIENCE_EVENT_BYTES, EXPERIENCE_PAGE_BYTES,
  type ExperienceBinding, type ExperienceInput, type ExperienceEvent } from './experience-contract.js';
import { appendExperience, eventFromRow, experienceTransaction, traceRows, traceEvents, requireTrace,
  experienceCursor, encodeExperienceCursor, experienceHash } from './experience-journal.js';

export { experienceInputSchema } from './experience-contract.js';
type ParsedInput = ReturnType<typeof experienceInputSchema.parse>;
interface TraceSummary { trace_id: string; title: string; event_count: number; state: 'open' | 'sealed'; archive_status: 'pending' | 'available'; sequence: number }
interface Difference { ordinal: number; left: ExperienceEvent | null; right: ExperienceEvent | null }
export interface ExperienceResult {
  ok: true;
  trace_id?: string;
  event?: ExperienceEvent;
  events?: ExperienceEvent[];
  traces?: TraceSummary[];
  differences?: Difference[];
  basis?: 'recorded-facts';
  archive?: ExperienceArchiveReceipt | { status: 'unavailable'; durable: false; reason: string };
  state?: 'open' | 'sealed';
  partial?: boolean;
  next?: { operation: 'history.experience'; params: ExperienceInput } | null;
}
function summary(db: DatabaseSync, workspace: string, trace: string, snapshot: number): TraceSummary {
  const rows = traceRows(db, workspace, trace, snapshot);
  const events = traceEvents(rows);
  return { trace_id: trace, title: events[0]!.title, event_count: events.length,
    state: rows.some(row => row.event_type === 'experience.sealed') ? 'sealed' : 'open',
    archive_status: rows.some(row => row.event_type === 'experience.archive') ? 'available' : 'pending', sequence: events[0]!.sequence };
}
function paged<T>(items: T[], limit: number): T[] {
  const selected: T[] = [];
  let bytes = 0;
  for (const item of items) {
    const size = Buffer.byteLength(JSON.stringify(item));
    if (selected.length >= limit || bytes + size > EXPERIENCE_PAGE_BYTES) break;
    selected.push(item); bytes += size;
  }
  if (items.length && !selected.length) throw new Error('EXPERIENCE_ITEM_LIMIT: item exceeds the response byte budget');
  return selected;
}
function next(input: ParsedInput, cursor: ReturnType<typeof experienceCursor>, after: number, hasMore: boolean): ExperienceResult['next'] {
  return hasMore ? { operation: 'history.experience', params: { ...input, cursor: encodeExperienceCursor({ ...cursor, after }) } as ExperienceInput } : null;
}

/** All SQLite transactions end before any optional archive IO is awaited. */
export async function executeExperience(db: DatabaseSync, bound: ExperienceBinding, supplied: unknown): Promise<ExperienceResult> {
  const input = experienceInputSchema.parse(supplied);
  if (containsSecretLikeText(JSON.stringify(supplied))) throw new Error('EXPERIENCE_SECRET_REJECTED: secret-like content cannot be recorded');
  const binding = { ...bound, workspace: resolve(bound.workspace) };
  if (!binding.actorId.trim() || binding.actorId.length > 128 || (binding.sessionId?.length ?? 0) > 128
    || containsSecretLikeText(JSON.stringify(binding))) throw new Error('EXPERIENCE_ACTOR_REQUIRED: bounded nonsecret actor identity is required');
  if (input.action === 'record') return experienceTransaction(db, () => {
    const { action: _action, ...content } = input;
    const event = { ...content, anchors: normalizeAnchors(binding.workspace, content.anchors) };
    if (Buffer.byteLength(JSON.stringify(event)) > MAX_EXPERIENCE_EVENT_BYTES) throw new Error('EXPERIENCE_EVENT_LIMIT: event exceeds 8192 bytes');
    const rows = traceRows(db, binding.workspace, event.trace_id);
    const exists = traceEvents(rows).some(item => item.event_id === event.event_id);
    if (!exists && rows.some(row => row.event_type === 'experience.sealed')) throw new Error('EXPERIENCE_SEALED: trace is immutable after seal');
    if (!exists && traceEvents(rows).length >= MAX_EXPERIENCE_EVENTS) throw new Error('EXPERIENCE_TRACE_LIMIT: start a related trace after 128 events');
    const row = appendExperience(db, binding, event.trace_id, `event:${event.event_id}`, 'experience.record', event);
    return { ok: true, trace_id: event.trace_id, event: eventFromRow(row) };
  });
  if (input.action === 'seal') {
    const rows = experienceTransaction(db, () => {
      const existing = traceRows(db, binding.workspace, input.trace_id); requireTrace(existing);
      if (!existing.some(row => row.event_type === 'experience.sealed')) appendExperience(db, binding, input.trace_id,
        'sealed', 'experience.sealed', { digest: experienceHash(traceEvents(existing)) });
      return traceRows(db, binding.workspace, input.trace_id);
    });
    try {
      const archive = await archiveExperience(db, binding.workspace, input.trace_id, traceEvents(rows));
      experienceTransaction(db, () => {
        const existing = traceRows(db, binding.workspace, input.trace_id).find(row => row.event_type === 'experience.archive');
        if (existing) {
          if (!isDeepStrictEqual(JSON.parse(existing.payload_json), archive)) throw new Error('EXPERIENCE_ARCHIVE_CONFLICT: archive receipt differs');
        } else appendExperience(db, binding, input.trace_id, 'archive', 'experience.archive', archive);
      });
      return { ok: true, trace_id: input.trace_id, state: 'sealed', archive };
    } catch {
      return { ok: true, trace_id: input.trace_id, state: 'sealed', archive: {
        status: 'unavailable', durable: false, reason: 'Archive unavailable; SQLite trace remains readable. Retry seal to repair publication.' } };
    }
  }
  const highWater = workspaceEventHighWater(db, binding.workspace);
  const { cursor: _rawCursor, limit: _limit, ...selection } = input;
  const cursor = experienceCursor(input.cursor, { workspace: binding.workspace, ...selection }, highWater);
  const limit = input.limit ?? 3;
  if (input.action === 'list' || input.action === 'recover') {
    const pending = input.action === 'recover' ? `AND NOT EXISTS (SELECT 1 FROM event_outbox a
      WHERE a.workspace_path = e.workspace_path AND a.aggregate_kind = 'experience' AND a.aggregate_id = e.aggregate_id
      AND a.event_type = 'experience.archive' AND a.sequence <= ?)` : '';
    const args = [binding.workspace, cursor.snapshot, ...(input.action === 'recover' ? [cursor.snapshot] : []), cursor.after, limit + 1];
    const records = db.prepare(`SELECT aggregate_id AS trace_id, MIN(sequence) AS first_sequence FROM event_outbox e
      WHERE workspace_path = ? AND aggregate_kind = 'experience' AND event_type = 'experience.record' AND sequence <= ? ${pending}
      GROUP BY aggregate_id HAVING MIN(sequence) > ? ORDER BY MIN(sequence) LIMIT ?`).all(...args) as unknown as Array<{ trace_id: string; first_sequence: number }>;
    const traces = records.slice(0, limit).map(row => summary(db, binding.workspace, row.trace_id, cursor.snapshot));
    return { ok: true, traces, partial: records.length > limit,
      next: next(input, cursor, traces.at(-1)?.sequence ?? cursor.after, records.length > limit) };
  }
  const rows = traceRows(db, binding.workspace, input.trace_id, cursor.snapshot); requireTrace(rows);
  if (input.action === 'compare') {
    const otherRows = traceRows(db, binding.workspace, input.other_trace_id, cursor.snapshot); requireTrace(otherRows);
    const left = traceEvents(rows); const right = traceEvents(otherRows);
    const differences: Difference[] = [];
    const facts = (event: ExperienceEvent | undefined) => {
      if (!event) return null;
      const { trace_id: _trace, event_id: _event, sequence: _sequence, created_at: _created,
        actor_id: _actor, session_id: _session, ...content } = event;
      return content;
    };
    for (let ordinal = cursor.after; ordinal < Math.max(left.length, right.length); ordinal++) {
      if (!isDeepStrictEqual(facts(left[ordinal]), facts(right[ordinal]))) {
        differences.push({ ordinal, left: left[ordinal] ?? null, right: right[ordinal] ?? null });
      }
    }
    const page = paged(differences, limit); const partial = page.length < differences.length;
    return { ok: true, basis: 'recorded-facts', differences: page, partial,
      next: next(input, cursor, (page.at(-1)?.ordinal ?? cursor.after - 1) + 1, partial) };
  }
  let events = traceEvents(rows);
  const archiveRow = rows.find(row => row.event_type === 'experience.archive');
  if (input.source === 'archive') {
    if (!archiveRow) return { ok: true, trace_id: input.trace_id, archive: { status: 'unavailable', durable: false,
      reason: 'No durable archive receipt exists; read source journal or retry seal.' } };
    try { events = await readExperienceArchive(db, binding.workspace, input.trace_id, JSON.parse(archiveRow.payload_json) as ExperienceArchiveReceipt); }
    catch { return { ok: true, trace_id: input.trace_id, archive: { status: 'unavailable', durable: false,
      reason: 'Archive evidence cannot be read or verified; the SQLite journal remains available.' } }; }
  }
  const remaining = events.filter(event => event.sequence > cursor.after);
  const page = paged(remaining, limit); const partial = page.length < remaining.length;
  return { ok: true, trace_id: input.trace_id, events: page,
    state: rows.some(row => row.event_type === 'experience.sealed') ? 'sealed' : 'open', partial,
    next: next(input, cursor, page.at(-1)?.sequence ?? cursor.after, partial) };
}
