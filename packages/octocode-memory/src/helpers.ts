/**
 * helpers.ts — Pure utility functions.
 * No I/O, no globals, no side effects.
 */

import { resolve } from 'node:path';
import type { MemoryRecord, MemoryRow } from './types.js';

// ─── Valid labels ─────────────────────────────────────────────────────────────

export const MEMORY_LABELS = new Set([
  'BUG', 'FEATURE', 'SUGGESTION', 'GOTCHA', 'IMPROVEMENT', 'DECISION',
  'ARCHITECTURE', 'SECURITY', 'PERFORMANCE', 'TEST', 'BUILD', 'DOCS',
  'CONFIG', 'WORKFLOW', 'REFACTOR', 'API', 'RELEASE', 'INCIDENT',
  'EXPERIENCE', // post-task reflections (worked/partial/failed outcomes)
  'OTHER',
]);

export const REFLECTION_IMPORTANCE: Record<string, number> = {
  failed: 8,
  partial: 6,
  worked: 5,
};

// ─── Time ─────────────────────────────────────────────────────────────────────

/** Current UTC timestamp as ISO-8601 (milliseconds stripped). */
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

/** Normalize a label string to a valid MEMORY_LABELS value (defaults to OTHER). */
export function normalizeLabel(value: unknown): string {
  if (!value) return 'OTHER';
  const cleaned = String(value).trim().toUpperCase().replace(/[\s-]+/g, '_');
  return MEMORY_LABELS.has(cleaned) ? cleaned : 'OTHER';
}

/** Resolve and normalize a file path to absolute. Returns null for falsy input. */
export function normalizeFilePath(filePath: unknown): string | null {
  if (!filePath) return null;
  return resolve(String(filePath));
}

// ─── Row-to-shape serializer ──────────────────────────────────────────────────

/** Serialize a raw DB row to a clean MemoryRecord. */
export function rowToMemory(row: MemoryRow): MemoryRecord {
  return {
    memory_id: row.memory_id,
    agent_id: row.agent_id,
    task_context: row.task_context,
    observation: row.observation,
    importance_score: row.importance_score,
    state: (row.state as 'ACTIVE' | 'SUPERSEDED') ?? 'ACTIVE',
    label: row.label ?? 'OTHER',
    superseded_by: row.superseded_by ?? null,
    tags: parseJsonList(row.tags_json),
    references: parseJsonList(row.references_json),
    workspace_path: row.workspace_path ?? null,
    repo: row.repo ?? null,
    ref: row.ref ?? null,
    file: row.file ?? null,
    novelty_score: row.novelty_score ?? null,
    similar_memory_ids: parseJsonList(row.similar_memory_ids_json),
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
