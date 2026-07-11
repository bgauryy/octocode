/**
 * helpers.ts — Pure utility functions.
 * No I/O, no globals, no side effects.
 */

import { resolve } from 'node:path';
import type { MemoryRecord, MemoryRow } from './types.js';

// ─── Valid labels ─────────────────────────────────────────────────────────────

export const MEMORY_LABEL_VALUES = [
  'BUG', 'FEATURE', 'SUGGESTION', 'GOTCHA', 'IMPROVEMENT', 'DECISION',
  'ARCHITECTURE', 'SECURITY', 'PERFORMANCE', 'TEST', 'BUILD', 'DOCS',
  'CONFIG', 'WORKFLOW', 'REFACTOR', 'API', 'RELEASE', 'INCIDENT',
  'EXPERIENCE', // post-task reflections (worked/partial/failed outcomes)
  'OVERRIDE',   // contradicts model training defaults (e.g. "this repo uses Bun, not npm")
  'OTHER',
] as const;

export const MEMORY_LABELS = new Set<string>(MEMORY_LABEL_VALUES);

export const REFLECTION_IMPORTANCE: Record<string, number> = {
  failed: 8,
  partial: 6,
  worked: 5,
};

// ─── Time ─────────────────────────────────────────────────────────────────────

/** Current UTC timestamp as ISO-8601 (milliseconds stripped — a deliberate,
 *  test-enforced contract; see tests/helpers.test.ts). */
export function utcNow(): string {
  return new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
}

// ─── JSON list helpers ────────────────────────────────────────────────────────

/** Safely parse a JSON array field; returns [] on any failure. */
export function parseJsonList(value: unknown): string[] {
  if (!value) return [];
  if (Array.isArray(value)) return (value as unknown[]).map(String).filter(Boolean);
  try {
    const parsed: unknown = JSON.parse(String(value));
    return Array.isArray(parsed) ? (parsed as unknown[]).map(String).filter(Boolean) : [];
  } catch {
    return [];
  }
}

/** Comma-surrounded tags string for LIKE searches: `,tag1,tag2,` */
export function tagsText(tags: string[]): string {
  return tags.length === 0 ? ',' : ',' + tags.join(',') + ',';
}

// ─── Normalizers ─────────────────────────────────────────────────────────────

/**
 * Deduplicate and clean tag strings.
 * @param tags   Array of raw tag values
 * @param csv    Comma-separated additional tags
 */
export function normalizeTags(tags: string[] = [], csv = ''): string[] {
  const raw = [...tags];
  if (csv) raw.push(...csv.split(','));
  const seen = new Set<string>();
  const out: string[] = [];
  for (const t of raw) {
    const cleaned = t.trim().toLowerCase().replace(/[^a-zA-Z0-9_.:-]+/g, '-').replace(/^-|-$/g, '');
    if (cleaned && !seen.has(cleaned)) {
      out.push(cleaned);
      seen.add(cleaned);
    }
  }
  return out;
}

/** Deduplicate and truncate references. */
export function normalizeReferences(refs: string[] = []): string[] {
  const seen = new Set<string>();
  return refs
    .map(r => (r ?? '').trim().slice(0, 512))
    .filter(r => r && !seen.has(r) && seen.add(r))
    .slice(0, 20);
}

/**
 * Normalize a label string to a valid MEMORY_LABELS value.
 * Empty/missing → OTHER. Unknown labels hard-error.
 */
export function normalizeLabel(value: unknown): string {
  if (value == null || String(value).trim() === '') return 'OTHER';
  const cleaned = String(value).trim().toUpperCase().replace(/[\s-]+/g, '_');
  if (MEMORY_LABELS.has(cleaned)) return cleaned;
  throw new Error(`invalid label "${String(value)}"; allowed: ${MEMORY_LABEL_VALUES.join(', ')}`);
}

export const NOTIFICATION_KIND_VALUES = [
  'claim', 'handoff', 'question', 'reply', 'blocker', 'request', 'decision', 'fyi',
] as const;

export const NOTIFICATION_KINDS = new Set<string>(NOTIFICATION_KIND_VALUES);

/** Validate signal kind; unknown kinds hard-error. */
export function normalizeNotificationKind(value: unknown): (typeof NOTIFICATION_KIND_VALUES)[number] {
  const cleaned = String(value ?? '').trim().toLowerCase();
  if (NOTIFICATION_KINDS.has(cleaned)) {
    return cleaned as (typeof NOTIFICATION_KIND_VALUES)[number];
  }
  throw new Error(
    `invalid signal kind "${String(value)}"; allowed: ${NOTIFICATION_KIND_VALUES.join(', ')}`,
  );
}

export const REFLECTION_OUTCOME_VALUES = ['worked', 'partial', 'failed'] as const;

/** Validate reflect outcome; unknown outcomes hard-error. */
export function normalizeReflectionOutcome(value: unknown): (typeof REFLECTION_OUTCOME_VALUES)[number] {
  // Missing outcome still defaults to partial; only non-empty unknown values hard-error.
  if (value == null || String(value).trim() === '') return 'partial';
  const cleaned = String(value).trim().toLowerCase();
  if ((REFLECTION_OUTCOME_VALUES as readonly string[]).includes(cleaned)) {
    return cleaned as (typeof REFLECTION_OUTCOME_VALUES)[number];
  }
  throw new Error(`invalid outcome "${String(value)}"; allowed: ${REFLECTION_OUTCOME_VALUES.join('|')}`);
}

/** Resolve and normalize a file path to absolute. Returns null for falsy input. */
export function normalizeFilePath(filePath: unknown, cwd?: string): string | null {
  if (!filePath) return null;
  const p = String(filePath);
  return cwd ? resolve(cwd, p) : resolve(p);
}

/** Normalize the optional workspace sub-scope (package/service/artifact name). */
export function normalizeArtifact(value: unknown): string | null {
  if (value == null) return null;
  const cleaned = String(value).trim().slice(0, 256);
  return cleaned.length > 0 ? cleaned : null;
}

// ─── Row-to-shape serializer ──────────────────────────────────────────────────

/** Serialize a raw DB row to a clean MemoryRecord. */
export function rowToMemory(row: MemoryRow): MemoryRecord {
  return {
    memory_id: row.memory_id,
    agent_id: row.agent_id,
    task_context: row.task_context,
    observation: row.observation,
    importance: row.importance,
    state: (row.state as 'ACTIVE' | 'SUPERSEDED') ?? 'ACTIVE',
    label: row.label ?? 'OTHER',
    superseded_by: row.superseded_by ?? null,
    tags: parseJsonList(row.tags_json),
    // references are stored in memory_refs table; populated separately via JOIN
    references: [],
    workspace_path: row.workspace_path ?? null,
    artifact: row.artifact ?? null,
    repo: row.repo ?? null,
    ref: row.ref ?? null,
    novelty_score: row.novelty_score ?? null,
    failure_signature: row.failure_signature ?? null,
    access_count: row.access_count ?? 0,
    last_accessed_at: row.last_accessed_at ?? null,
    decay_half_life_days: row.decay_half_life_days ?? null,
    valid_from: row.valid_from ?? null,
    valid_to: row.valid_to ?? null,
    expired_at: row.expired_at ?? null,
    file_tree_fingerprint: row.file_tree_fingerprint ?? null,
    created_at: row.created_at,
    updated_at: row.updated_at ?? null,
  };
}

// ─── Token-lean CLI projections ───────────────────────────────────────────────

/** Flatten whitespace and truncate for agent-facing summaries. */
export function summarizeText(value: string, max: number): string {
  const flat = value.replace(/\s+/g, ' ').trim();
  if (flat.length <= max) return flat;
  return flat.slice(0, Math.max(0, max - 3)).trimEnd() + '...';
}

/** Default memory recall projection — ids + routing fields, not full rows. */
export function projectMemoryLean(memory: MemoryRecord): Record<string, unknown> {
  const result: Record<string, unknown> = {
    memory_id: memory.memory_id,
    label: memory.label,
    importance: memory.importance,
    task_context: summarizeText(memory.task_context ?? '', 120),
    observation: summarizeText(memory.observation ?? '', 200),
  };
  const tags = memory.tags ?? [];
  if (tags.length > 0) result['tags'] = tags.slice(0, 3);
  if (tags.length > 3) {
    result['tag_count'] = tags.length;
    result['tag_omitted_count'] = tags.length - 3;
  }
  const references = memory.references ?? [];
  if (references.length > 0) result['references'] = references.slice(0, 3);
  if (references.length > 3) {
    result['reference_count'] = references.length;
    result['reference_omitted_count'] = references.length - 3;
  }
  if (typeof memory.score === 'number' && Number.isFinite(memory.score)) {
    result['score'] = Math.round(memory.score * 10_000) / 10_000;
  }
  if (memory.failure_signature) result['failure_signature'] = memory.failure_signature;
  return result;
}
