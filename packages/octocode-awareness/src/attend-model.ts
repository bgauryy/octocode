/**
 * attend.ts - bounded agent start packet over awareness state.
 */
import { existsSync, readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { type AwarenessQueryRow } from './repo-context.js';

export interface AttendParams {
  agentId?: string | null;
  agent_id?: string | null;
  workspacePath?: string | null;
  workspace_path?: string | null;
  workspace?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  query?: string | null;
  file?: string | string[] | null;
  limit?: number | null;
  compact?: boolean | null;
  includeBodies?: boolean | null;
  include_bodies?: boolean | null;
  explainOrgan?: boolean | null;
  explain_organ?: boolean | null;
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
  profile?: Record<string, number>;
  organ_state?: Record<string, unknown>;
  drive_state?: Record<string, unknown>;
  workboard: Record<string, AwarenessQueryRow[]>;
  evidence: AttendEvidence[];
  gaps?: string[];
  bloat_warnings?: string[];
  verification_targets?: AwarenessQueryRow[];
  trust_warnings?: string[];
  trace?: Array<{ step: string; count?: number; note?: string }>;
  organ_reference?: Array<{ organ: string; role: string; commands: string[]; guardrail: string }>;
  next: string;
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
      'path', 'peer_count', 'omitted_peer_count', 'locked', 'lock_agent_id',
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

export function lineCount(path: string): number | null {
  if (!existsSync(path)) return null;
  try {
    return readFileSync(path, 'utf8').split(/\r?\n/).length;
  } catch {
    return null;
  }
}

export function projectionStats(workspacePath: string): Array<{ file: string; lines: number | null; mtime_ms: number | null }> {
  return ['AGENTS.md', 'KNOWLEDGE.md', join('awareness', 'manifest.json')].map(file => {
    const path = join(workspacePath, '.octocode', file);
    let mtimeMs: number | null = null;
    try { mtimeMs = existsSync(path) ? statSync(path).mtimeMs : null; } catch { /* ignore projection stat errors */ }
    return { file: `.octocode/${file.replace(/\\/g, '/')}`, lines: lineCount(path), mtime_ms: mtimeMs };
  });
}

export function manifestWarnings(
  workspacePath: string,
  stats: Array<{ file: string; mtime_ms: number | null }>,
  liveSourceRevision: string | (() => string),
): string[] {
  const manifestPath = join(workspacePath, '.octocode', 'awareness', 'manifest.json');
  if (!existsSync(manifestPath)) return ['.octocode/awareness/manifest.json missing; run wiki sync when projection context is needed'];
  try {
    const manifest = JSON.parse(readFileSync(manifestPath, 'utf8')) as {
      generated_at?: string;
      files?: string[];
      source?: { revision?: string };
      completeness?: Record<string, { is_partial?: boolean; omitted_count?: number | null }>;
      budgets?: { markdown?: Record<string, { within_budget?: boolean }> };
    };
    const warnings: string[] = [];
    const files = manifest.files ?? [];
    const missingManagedCount = files.filter(file => !existsSync(resolve(workspacePath, file))).length;
    if (missingManagedCount > 0) warnings.push(`manifest has ${missingManagedCount} missing generated file(s); regenerate repo projection`);
    const markdownBudgets = manifest.budgets?.markdown ?? {};
    for (const [file, budget] of Object.entries(markdownBudgets)) {
      if (budget.within_budget === false) warnings.push(`manifest budget exceeded for ${file}`);
    }
    const partialSections = Object.values(manifest.completeness ?? {}).filter(section => section.is_partial);
    if (partialSections.length > 0) {
      warnings.push(`manifest is a partial snapshot (${partialSections.length} section(s)); use live SQLite for omitted rows`);
    } else if (!manifest.source?.revision) {
      warnings.push('manifest missing source revision; regenerate repo projection');
    } else if (manifest.source.revision !== (typeof liveSourceRevision === 'function' ? liveSourceRevision() : liveSourceRevision)) {
      warnings.push('manifest source revision differs from live SQLite; regenerate repo projection');
    }
    if (manifest.generated_at) {
      const generatedMs = new Date(manifest.generated_at).getTime();
      if (Number.isFinite(generatedMs) && stats.some(stat => stat.file !== '.octocode/awareness/manifest.json' && stat.mtime_ms != null && stat.mtime_ms > generatedMs + 1000)) {
        warnings.push('manifest older than generated projection files; regenerate repo projection');
      }
    }
    return warnings;
  } catch {
    return ['.octocode/awareness/manifest.json unreadable; regenerate repo projection'];
  }
}

export function projectionWarnings(
  workspacePath: string,
  stats: Array<{ file: string; lines: number | null; mtime_ms: number | null }>,
  liveSourceRevision: string | (() => string),
): string[] {
  const budgets: Record<string, number> = {
    '.octocode/AGENTS.md': 80,
    '.octocode/KNOWLEDGE.md': 200,
  };
  const markdownWarnings = stats.flatMap(stat => {
    const budget = budgets[stat.file];
    if (stat.file === '.octocode/KNOWLEDGE.md' && stat.lines == null) return [];
    if (stat.lines == null) return [`${stat.file} missing; run wiki sync when projection context is needed`];
    if (budget != null && stat.lines > budget) return [`${stat.file} has ${stat.lines} lines over budget ${budget}`];
    return [];
  });
  return [...markdownWarnings, ...manifestWarnings(workspacePath, stats, liveSourceRevision)];
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
  if (/(awareness|homeostatic|attend|workboard|memory|wiki|task|reflection|drive|motivation|resource|creative|personality)/.test(haystack)) {
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

export function shellQuote(value: string): string {
  return `'${value.replace(/'/g, `'"'"'`)}'`;
}
