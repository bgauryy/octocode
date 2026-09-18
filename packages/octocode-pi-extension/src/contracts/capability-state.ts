import { z } from 'zod';
import { capabilityDefinitionRevision, type CapabilityKind } from './capability-sources.js';
import { utcNow, type ReadableSqlite, type SqliteLike } from '@octocodeai/octocode-awareness/host';

const keySchema = z.string().min(1).max(4096).refine(value => !value.includes('\0'));
const digestSchema = z.string().regex(/^sha256:[a-f0-9]{64}$/);
export const capabilitySourceSchema = z.object({
  kind: z.enum(['skill', 'mcp', 'hook']), sourceId: digestSchema, revision: digestSchema,
  name: keySchema, path: keySchema,
}).strict();
export type CapabilitySource = z.infer<typeof capabilitySourceSchema>;
export const capabilitySourceReviewSchema = capabilitySourceSchema.extend({
  scopeKey: keySchema, enabled: z.boolean(), reviewedAt: z.iso.datetime(),
}).strict();
export type CapabilitySourceReview = z.infer<typeof capabilitySourceReviewSchema>;
export type CapabilitySourceStatus = 'active' | 'disabled' | 'pending-review' | 'unavailable' | 'untrusted';

const GLOBAL_SCOPE = '*';
const reviewPrefix = (scopeKey: string) => `capability-review:v1:${capabilityDefinitionRevision(keySchema.parse(scopeKey))}:`;
const reviewKey = (scopeKey: string, sourceId: string) => `${reviewPrefix(scopeKey)}${digestSchema.parse(sourceId)}`;
const selectionKey = (scopeKey: string, kind: CapabilityKind, name: string) => `capability-selection:v1:${capabilityDefinitionRevision([keySchema.parse(scopeKey), kind, keySchema.parse(name).replace(/\s+/g, ' ').trim().toLowerCase()])}`;

function readValue(db: ReadableSqlite, key: string): unknown {
  const row = db.prepare('SELECT value FROM octocode_meta WHERE key=?').get(key) as { value?: unknown } | undefined;
  if (typeof row?.value !== 'string') return undefined;
  try { return JSON.parse(row.value) as unknown; } catch { return undefined; }
}
function writeValue(db: SqliteLike, key: string, value: unknown): void {
  db.prepare(`INSERT INTO octocode_meta (key, value, updated_at) VALUES (?, ?, ?)
    ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at`).run(key, JSON.stringify(value), utcNow());
}

export function getCapabilitySourceReview(db: ReadableSqlite, scopeKey: string, sourceId: string): CapabilitySourceReview | undefined {
  for (const scope of new Set([scopeKey, GLOBAL_SCOPE])) {
    const result = capabilitySourceReviewSchema.safeParse(readValue(db, reviewKey(scope, sourceId)));
    if (result.success && result.data.scopeKey === scope && result.data.sourceId === sourceId) return result.data;
  }
  return undefined;
}

/** Only explicit review writes approval. Name-only enablement cannot authorize a linked source. */
export function reviewCapabilitySource(db: SqliteLike, scopeKey: string, source: CapabilitySource, options: { enabled?: boolean; select?: boolean } = {}): CapabilitySourceReview {
  const review = capabilitySourceReviewSchema.parse({ ...capabilitySourceSchema.parse(source), scopeKey, enabled: options.enabled ?? true, reviewedAt: utcNow() });
  writeValue(db, reviewKey(scopeKey, source.sourceId), review);
  if (options.select) selectCapabilitySource(db, scopeKey, source.kind, source.name, source.sourceId);
  return review;
}

export function setCapabilitySourceEnabled(db: ReadableSqlite, scopeKey: string, sourceId: string, enabled: boolean): void {
  const review = getCapabilitySourceReview(db, scopeKey, sourceId);
  if (!review) throw new Error('Review the capability source before enabling it');
  writeValue(db, reviewKey(scopeKey, sourceId), capabilitySourceReviewSchema.parse({ ...review, scopeKey, enabled }));
}

export function selectCapabilitySource(db: SqliteLike, scopeKey: string, kind: CapabilityKind, name: string, sourceId: string | undefined): void {
  const key = selectionKey(scopeKey, kind, name);
  if (sourceId === undefined) { writeValue(db, key, null); return; }
  writeValue(db, key, digestSchema.parse(sourceId));
}

export function getSelectedCapabilitySource(db: ReadableSqlite, scopeKey: string, kind: CapabilityKind, name: string): string | undefined {
  for (const scope of new Set([scopeKey, GLOBAL_SCOPE])) {
    const value = readValue(db, selectionKey(scope, kind, name));
    if (value === null) return undefined;
    const result = digestSchema.safeParse(value);
    if (result.success) return result.data;
  }
  return undefined;
}

export function getCapabilitySourceStatus(db: ReadableSqlite, scopeKey: string, source: CapabilitySource & { available?: boolean; trusted?: boolean; defaultEnabled?: boolean }): CapabilitySourceStatus {
  if (source.available === false) return 'unavailable';
  if (source.trusted === false) return 'untrusted';
  const review = getCapabilitySourceReview(db, scopeKey, source.sourceId);
  if (!review) return source.defaultEnabled ? 'active' : 'disabled';
  if (review.kind !== source.kind || review.path !== source.path || review.revision !== source.revision) return 'pending-review';
  return review.enabled ? 'active' : 'disabled';
}

/** Includes removed source reviews so hosts can project them as unavailable. */
export function listCapabilitySourceReviews(db: ReadableSqlite, scopeKey: string): CapabilitySourceReview[] {
  const reviews = new Map<string, CapabilitySourceReview>();
  for (const scope of new Set([GLOBAL_SCOPE, scopeKey])) {
    const rows = db.prepare('SELECT value FROM octocode_meta WHERE key LIKE ? ORDER BY key').all(`${reviewPrefix(scope)}%`) as Array<{ value: string }>;
    for (const row of rows) {
      try {
        const result = capabilitySourceReviewSchema.safeParse(JSON.parse(row.value));
        if (result.success && result.data.scopeKey === scope) reviews.set(result.data.sourceId, result.data);
      } catch { /* Malformed state never grants authority. */ }
    }
  }
  return [...reviews.values()];
}
