/**
 * Bounded agent observation packet over Awareness state.
 */
import type { AttendNext } from './attend-flow.js';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { AwarenessQueryRow } from './repo-model.js';
import type { OperationalState, Regulation, RuntimeObservation } from './attend-physiology.js';

export interface AttendParams {
  runtimeObservation?: RuntimeObservation;
  agentId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  query?: string | null;
  file?: string | string[] | null;
  limit?: number | null;
  compact?: boolean | null;
  includeBodies?: boolean | null;
  explainOrgan?: boolean | null;
  cwd?: string | null;
}

export interface AttendEvidence {
  kind: 'memory';
  id: string;
  label: string;
  importance: number;
  title: string;
  summary: string;
  references: string[];
  reference_count?: number;
  omitted_reference_count?: number;
  why_selected: string[];
  trust: 'existing_file_lead' | 'needs_refs' | 'generated_or_external_lead';
}

export interface AttendResult {
  ok: true;
  generated_at: string;
  workspace_path: string;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  counts?: Record<string, number>;
  operational_state: OperationalState;
  regulation: Regulation;
  profile?: Record<string, number>;
  organ_state?: Record<string, unknown>;
  drive_state?: Record<string, unknown>;
  workboard: Record<string, AwarenessQueryRow[]>;
  evidence: AttendEvidence[];
  gaps?: string[];
  verification_targets?: AwarenessQueryRow[];
  trust_warnings?: string[];
  trace?: Array<{ step: string; count?: number; note?: string }>;
  organ_reference?: Array<{ organ: string; role: string; commands: string[]; guardrail: string }>;
  next: AttendNext;
}

export const TEAM_NORMS = [
  'evidence-first',
  'bounded',
  'cooperative',
  'non-destructive',
  'verify-before-policy',
];

export const ORGAN_REFERENCE = [
  {
    organ: 'senses',
    role: 'read live state',
    commands: ['workspace status', 'query repo-profile'],
    guardrail: 'Live DB beats stale projections.',
  },
  {
    organ: 'attention',
    role: 'select a small packet',
    commands: ['attend', 'query workboard', 'memory recall'],
    guardrail: 'Show gaps, not dumps.',
  },
  {
    organ: 'memory',
    role: 'durable lessons',
    commands: ['memory record', 'memory recall', 'reflect record'],
    guardrail: 'Memories are leads until verified.',
  },
  {
    organ: 'immune_pruning',
    role: 'tag weak/stale evidence',
    commands: ['memory forget --dry-run', 'maintenance digest --dry-run', 'query workboard'],
    guardrail: 'Report before deleting.',
  },
  {
    organ: 'corpus_bridge',
    role: 'coordinate agents',
    commands: ['plan list', 'task ready', 'task claim', 'work start', 'work list', 'signal publish', 'lock acquire', 'verify audit'],
    guardrail: 'SQLite is canonical.',
  },
  {
    organ: 'drive',
    role: 'goal/gaps/resources',
    commands: ['attend --explain-organ', 'query workboard'],
    guardrail: 'Collective state, not persona.',
  },
];

export function limitOf(value: number | null | undefined, fallback = 10, max = 50): number {
  if (value == null || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(1, Math.floor(value)));
}

export function stringList(value: string | string[] | null | undefined): string[] {
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  if (value == null || value === '') return [];
  return [String(value)];
}

export function summarize(value: string, max: number): string {
  const flat = value.replace(/\s+/g, ' ').trim();
  if (flat.length <= max) return flat;
  return flat.slice(0, Math.max(0, max - 3)).trimEnd() + '...';
}

export function profileMap(rows: AwarenessQueryRow[]): Record<string, number> {
  return Object.fromEntries(rows.map(row => [String(row['metric']), Number(row['count'] ?? 0)]));
}

export function groupWorkboard(rows: AwarenessQueryRow[]): Record<string, AwarenessQueryRow[]> {
  const groups: Record<string, AwarenessQueryRow[]> = {};
  for (const row of rows) {
    const column = String(row['column'] ?? 'Other');
    const list = groups[column] ?? [];
    list.push(row);
    groups[column] = list;
  }
  return groups;
}

export function compactRow(row: AwarenessQueryRow): AwarenessQueryRow {
  if (row['item_type'] === 'file') {
    // Compact lobby: path + peer pressure + exclusivity only (drill with work list/show).
    return Object.fromEntries([
      'path', 'peer_count', 'omitted_peer_count', 'locked', 'lock_agent',
    ].flatMap(key => row[key] == null ? [] : [[key, row[key]]])) as AwarenessQueryRow;
  }
  const next: AwarenessQueryRow = {};
  for (const key of ['item_type', 'id', 'plan_id', 'status', 'agent_id', 'priority']) {
    const value = row[key];
    if (value != null) next[key] = value;
  }
  if (typeof row['title'] === 'string') next['title'] = summarize(row['title'], 60);
  return next;
}

export function compactWorkboard(grouped: Record<string, AwarenessQueryRow[]>, limit: number): Record<string, AwarenessQueryRow[]> {
  const actionable = ['Inbox', 'Ready', 'Claimed', 'Verify', 'FilesUnderWork', 'Maintenance'];
  return Object.fromEntries(actionable.flatMap(column => {
    const rows = grouped[column] ?? [];
    return rows.length === 0 ? [] : [[column, rows.slice(0, limit).map(compactRow)]];
  }));
}

export function uniqueStrings(values: string[]): string[] {
  return [...new Set(values.filter(Boolean))];
}

export function evidenceTrust(references: string[], workspacePath: string): AttendEvidence['trust'] {
  if (references.length === 0) return 'needs_refs';
  const missingFileReference = references.some(reference => {
    if (!reference.startsWith('file:')) return false;
    const rawPath = reference.slice('file:'.length).replace(/(?::\d+(?::\d+)?|#L\d+(?:-L?\d+)?)$/, '');
    const path = rawPath.startsWith('/') ? rawPath : resolve(workspacePath, rawPath);
    return !existsSync(path);
  });
  if (missingFileReference) return 'needs_refs';
  if (references.some(ref => ref.includes('.octocode/') || ref.startsWith('http'))) return 'generated_or_external_lead';
  return 'existing_file_lead';
}

export function resourceLeads(query: string, workspacePath: string): Array<Record<string, string>> {
  const haystack = query.toLowerCase();
  const leads: Array<Record<string, string>> = [];
  const add = (source: string, why: string, verification = 'lead_to_verify') => {
    leads.push({ source, why, verification });
  };
  if (/(awareness|homeostatic|attend|workboard|memory|task|reflection|drive|motivation|resource|creative|personality)/.test(haystack)) {
    add(
      join(workspacePath, '.octocode', 'rfc', 'homeostatic-awareness-loop', 'RFC.md'),
      'RFC goals and decision for the awareness loop',
    );
    add(
      join(workspacePath, '.octocode', 'rfc', 'homeostatic-awareness-loop', 'IMPLEMENTATION.md'),
      'dependency-ordered build plan for workboard, attend, drive_state, and digest',
    );
    add(
      join(workspacePath, 'packages', 'octocode-awareness', 'skills', 'octocode-awareness', 'references', 'homeostatic-loop.md'),
      'compact agent-facing organ and drive map',
    );
  }
  if (/(role.?dialogue|self.?reflection|tutor|student|builder|tester|alter.?ego|debate|duo)/.test(haystack)) {
    add(
      join(workspacePath, 'packages', 'octocode-awareness', 'skills', 'octocode-awareness', 'references', 'self-reflection-dialogue.md'),
      'role-dialogue pattern for hard ideas without persona bloat',
    );
  }
  if (leads.length === 0) {
    add(join(workspacePath, '.octocode', 'AGENTS.md'), 'generated repo context entrypoint, if present');
    add(join(workspacePath, 'AGENTS.md'), 'workspace-level agent instructions');
  }
  return leads.slice(0, 4);
}

export function chooseMode(query: string, evidenceCount: number, verifyCount: number, gapCount: number): 'explore' | 'exploit' | 'mixed' {
  if (verifyCount > 0 && gapCount === 0) return 'exploit';
  if (evidenceCount === 0 || /(design|rfc|brainstorm|research|unknown|approach|why|how)/i.test(query)) return verifyCount > 0 ? 'mixed' : 'explore';
  return gapCount > 0 ? 'mixed' : 'exploit';
}
