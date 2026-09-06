import { randomUUID } from 'node:crypto';
import { awarenessDatabasePath, DEFAULT_AWARENESS_STORAGE_SCOPE, type AwarenessStorageScope } from '../storage-scope.js';

// ─── Full public surface for in-process external hosts ────────────────────────
// Hosts that embed Awareness as a library instead of spawning the `cli.js`
// bin reach every capability from this package root: the embedding helpers, the
// pre-edit lock gate, and the programmatic CLI/hook-install entrypoints.
import type {
AgentRecord,
AgentStatus,
HandoffNote,
LiteMessage,
MemoryItem,
Plan,
PlanStatus,
} from '@octocodeai/octocode-shared/entities';

export interface AwarenessOptions {
  workspace?: string;
  dbPath?: string;
  scope?: AwarenessStorageScope;
}

export interface AwarenessSchema {
  entities: Record<string, string[]>;
  commands: Record<string, string[]>;
}

export interface PlanRow {
  plan_id: string;
  title: string;
  goal: string | null;
  status: PlanStatus;
  source_kind: string | null;
  source_key: string | null;
  rfc_path: string | null;
  rfc_revision: string | null;
  created_at: string;
  updated_at: string;
}

export interface HandoffRow {
  handoff_id: string;
  agent_id: string;
  summary: string;
  files_json: string;
  created_at: string;
  cleared_at: string | null;
}

/** Canonical `awareness_memories` projection for the host facade. */
export interface CanonicalMemoryRow {
  memory_id: string;
  label: string;
  observation: string;
  tags_json?: string;
  tags?: string[];
  created_at: string;
  embedding?: Uint8Array | null;
  embedding_model?: string | null;
}

/** Canonical `awareness_agents` projection for registry/presence callers. */
export interface CanonicalAgentRow {
  agent_id: string;
  agent_name: string | null;
  role: string | null;
  status: AgentStatus;
  metadata_json: string;
  registered_at: string;
  last_seen_at: string;
}

/** Canonical `signals` + `signal_reads` projection for the host message facade. */
export interface CanonicalMessageRow {
  signal_id: string;
  from_agent: string;
  to_agent: string | null;
  subject: string;
  body: string | null;
  files_json: string;
  created_at: string;
  read_at?: string | null;
}

export function now(): string {
  return new Date().toISOString();
}

export function cutoffIso(ageMs: number): string {
  if (!Number.isFinite(ageMs) || ageMs <= 0) throw new Error('age must be a positive duration');
  return new Date(Date.now() - ageMs).toISOString();
}

/**
 * Default presence window for counting "present" agents in status() when the
 * caller does not pass staleAfterMs. 30 min matches the lock/work default TTL,
 * so a crashed agent that never called leave ages out of the presence count.
 */
export const DEFAULT_AGENT_PRESENCE_MS = 30 * 60_000;

/**
 * Default cosine floor for semantic recall. 0 preserves the historical behavior
 * (keep any candidate with positive similarity) and matches the full
 * octocode-awareness search, which applies no hard floor and relies on ranking.
 * Callers that want to suppress weak matches — so a near-orthogonal vector can't
 * mask a better lexical result — pass a positive `minSimilarity` per recall.
 */
export const DEFAULT_SEMANTIC_MIN_SIMILARITY = 0;

export function id(prefix: string): string {
  return `${prefix}_${randomUUID().replace(/-/g, '').slice(0, 24)}`;
}

export function required(value: string | undefined | null, name: string): string {
  const trimmed = value?.trim() ?? '';
  if (!trimmed) throw new Error(`${name} is required`);
  return trimmed;
}

/** Resolve shared coordination storage at repository or global level. */
export function defaultDbPath(workspace: string, scope: AwarenessStorageScope = DEFAULT_AWARENESS_STORAGE_SCOPE): string {
  return awarenessDatabasePath(workspace, scope);
}

export function planFromRow(row: PlanRow): Plan {
  return {
    planId: row.plan_id,
    title: row.title,
    goal: row.goal,
    status: row.status,
    createdAt: row.created_at,
    updatedAt: row.updated_at,
    sourceKind: row.source_kind ?? null,
    sourceKey: row.source_key ?? null,
    rfcPath: row.rfc_path ?? null,
    rfcRevision: row.rfc_revision ?? null,
  };
}

export function handoffFromRow(row: HandoffRow): HandoffNote {
  return {
    handoffId: row.handoff_id,
    agentId: row.agent_id,
    summary: row.summary,
    files: JSON.parse(row.files_json) as string[],
    createdAt: row.created_at,
    clearedAt: row.cleared_at,
  };
}

export function memoryFromCanonicalRow(row: CanonicalMemoryRow): MemoryItem {
  return {
    memoryId: row.memory_id,
    label: row.label,
    text: row.observation,
    tags: row.tags ?? (row.tags_json ? JSON.parse(row.tags_json) as string[] : []),
    createdAt: row.created_at,
  };
}

export function agentFromCanonicalRow(row: CanonicalAgentRow): AgentRecord {
  return {
    agentId: row.agent_id,
    name: row.agent_name,
    role: row.role,
    status: row.status,
    metadata: JSON.parse(row.metadata_json || '{}') as Record<string, unknown>,
    createdAt: row.registered_at,
    lastSeenAt: row.last_seen_at,
  };
}

export function messageFromCanonicalSignalRow(row: CanonicalMessageRow): LiteMessage {
  return {
    messageId: row.signal_id,
    fromAgentId: row.from_agent,
    toAgentId: row.to_agent,
    topic: row.subject === 'message' ? null : row.subject,
    text: row.body ?? '',
    files: JSON.parse(row.files_json) as string[],
    createdAt: row.created_at,
    readAt: row.read_at ?? null,
  };
}

export function parseMetadata(metadata: string | Record<string, unknown> | undefined | null): Record<string, unknown> {
  if (!metadata) return {};
  if (typeof metadata !== 'string') return metadata;
  const trimmed = metadata.trim();
  if (!trimmed) return {};
  return JSON.parse(trimmed) as Record<string, unknown>;
}

export function splitTags(tags: string | string[] | undefined | null): string[] {
  if (Array.isArray(tags)) return tags.map((tag) => tag.trim()).filter(Boolean);
  return (tags ?? '').split(',').map((tag) => tag.trim()).filter(Boolean);
}

export function splitFiles(files: string | string[] | undefined | null): string[] {
  if (Array.isArray(files)) return files.map((file) => file.trim()).filter(Boolean);
  return (files ?? '').split(',').map((file) => file.trim()).filter(Boolean);
}

export function sleepMs(ms: number): void {
  if (ms <= 0) return;
  const buffer = new SharedArrayBuffer(4);
  Atomics.wait(new Int32Array(buffer), 0, 0, ms);
}

export function normalizeLeaseSeconds(value: number | undefined, fallback = 1800): number {
  if (value === undefined) return fallback;
  if (!Number.isFinite(value) || value <= 0) throw new Error('lease must be a finite positive duration');
  return Math.min(Math.max(Math.floor(value), 1), 3600);
}
