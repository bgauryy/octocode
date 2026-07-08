/**
 * attend.ts - read-only agent start packet over awareness state.
 */

import { existsSync, readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { getMemory } from './memory.js';
import { queryAwareness, type AwarenessQueryRow } from './repo-context.js';

export interface AttendParams {
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
  trust: 'verified_lead' | 'needs_refs' | 'generated_or_external_lead';
}

export interface AttendResult {
  ok: true;
  schema_version: 1;
  generated_at: string;
  workspace_path: string;
  artifact: string | null;
  repo: string | null;
  ref: string | null;
  profile: Record<string, number>;
  organ_state: Record<string, unknown>;
  drive_state: Record<string, unknown>;
  workboard: Record<string, AwarenessQueryRow[]>;
  evidence: AttendEvidence[];
  gaps: string[];
  bloat_warnings: string[];
  verification_targets: AwarenessQueryRow[];
  trust_warnings: string[];
  trace: Array<{ step: string; count?: number; note?: string }>;
  organ_reference?: Array<{ organ: string; role: string; commands: string[]; guardrail: string }>;
  next: string;
}

const TEAM_NORMS = [
  'evidence-first',
  'bounded',
  'cooperative',
  'non-destructive',
  'verify-before-policy',
];

const ORGAN_REFERENCE = [
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
    commands: ['signal publish', 'refinement set', 'lock acquire', 'verify audit'],
    guardrail: 'SQLite is canonical.',
  },
  {
    organ: 'drive',
    role: 'goal/gaps/resources',
    commands: ['attend --explain-organ', 'query workboard'],
    guardrail: 'Collective state, not persona.',
  },
];

function limitOf(value: number | null | undefined, fallback = 10, max = 50): number {
  if (value == null || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(1, Math.floor(value)));
}

function stringList(value: string | string[] | null | undefined): string[] {
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  if (value == null || value === '') return [];
  return [String(value)];
}

function summarize(value: string, max: number): string {
  const flat = value.replace(/\s+/g, ' ').trim();
  if (flat.length <= max) return flat;
  return flat.slice(0, Math.max(0, max - 3)).trimEnd() + '...';
}

function profileMap(rows: AwarenessQueryRow[]): Record<string, number> {
  return Object.fromEntries(rows.map(row => [String(row['metric']), Number(row['count'] ?? 0)]));
}

function groupWorkboard(rows: AwarenessQueryRow[]): Record<string, AwarenessQueryRow[]> {
  const groups: Record<string, AwarenessQueryRow[]> = {};
  for (const row of rows) {
    const column = String(row['column'] ?? 'Other');
    const list = groups[column] ?? [];
    list.push(row);
    groups[column] = list;
  }
  return groups;
}

function compactRow(row: AwarenessQueryRow): AwarenessQueryRow {
  const next: AwarenessQueryRow = {};
  for (const key of ['column', 'item_type', 'id', 'status', 'agent_id', 'quality', 'count', 'column_total', 'omitted_count', 'active_memories', 'tasks', 'open_refinements', 'open_signals']) {
    const value = row[key];
    if (value != null) next[key] = value;
  }
  if (typeof row['title'] === 'string') next['title'] = summarize(row['title'], 90);
  if (Array.isArray(row['reasons'])) next['reasons'] = row['reasons'].slice(0, 3);
  if (Array.isArray(row['files'])) {
    const files = row['files'];
    next['file_count'] = files.length;
    next['files'] = files.slice(0, 2);
    next['omitted_file_count'] = Math.max(0, files.length - 2);
  }
  if (Array.isArray(row['raw_ids'])) {
    const rawIds = row['raw_ids'];
    next['raw_id_count'] = rawIds.length;
    next['raw_ids'] = rawIds.slice(0, 5);
    next['omitted_raw_id_count'] = Math.max(0, rawIds.length - 5);
  }
  return next;
}

function compactWorkboard(grouped: Record<string, AwarenessQueryRow[]>, limit: number): Record<string, AwarenessQueryRow[]> {
  return Object.fromEntries(Object.entries(grouped).map(([column, rows]) => [
    column,
    rows.slice(0, limit).map(compactRow),
  ]));
}

function compactVerificationTarget(row: AwarenessQueryRow): AwarenessQueryRow {
  const compact = compactRow(row);
  return {
    id: compact['id'] ?? null,
    status: compact['status'] ?? null,
    title: compact['title'] ?? null,
    count: compact['count'] ?? null,
    raw_id_count: compact['raw_id_count'] ?? null,
    raw_ids: compact['raw_ids'] ?? [],
    column_total: compact['column_total'] ?? null,
    omitted_count: compact['omitted_count'] ?? null,
  };
}

function uniqueStrings(values: string[]): string[] {
  return [...new Set(values.filter(Boolean))];
}

function lineCount(path: string): number | null {
  if (!existsSync(path)) return null;
  try {
    return readFileSync(path, 'utf8').split(/\r?\n/).length;
  } catch {
    return null;
  }
}

function projectionStats(workspacePath: string): Array<{ file: string; lines: number | null; mtime_ms: number | null }> {
  return ['AGENTS.md', 'MEMORY.md', 'GOTCHAS.md', 'LEARN.md', 'BOOKMARKS.md', join('awareness', 'manifest.json')].map(file => {
    const path = join(workspacePath, '.octocode', file);
    let mtimeMs: number | null = null;
    try { mtimeMs = existsSync(path) ? statSync(path).mtimeMs : null; } catch { /* ignore projection stat errors */ }
    return { file: `.octocode/${file.replace(/\\/g, '/')}`, lines: lineCount(path), mtime_ms: mtimeMs };
  });
}

function manifestWarnings(workspacePath: string, stats: Array<{ file: string; mtime_ms: number | null }>): string[] {
  const manifestPath = join(workspacePath, '.octocode', 'awareness', 'manifest.json');
  if (!existsSync(manifestPath)) return ['.octocode/awareness/manifest.json missing; run repo inject when projection context is needed'];
  try {
    const manifest = JSON.parse(readFileSync(manifestPath, 'utf8')) as {
      generated_at?: string;
      files?: string[];
      budgets?: { markdown?: Record<string, { within_budget?: boolean }> };
    };
    const warnings: string[] = [];
    const files = manifest.files ?? [];
    if (!files.some(file => file.endsWith('/BOOKMARKS.md') || file.endsWith('\\BOOKMARKS.md') || file === 'BOOKMARKS.md')) {
      warnings.push('manifest missing BOOKMARKS.md; regenerate repo projection');
    }
    const markdownBudgets = manifest.budgets?.markdown ?? {};
    for (const [file, budget] of Object.entries(markdownBudgets)) {
      if (budget.within_budget === false) warnings.push(`manifest budget exceeded for ${file}`);
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

function projectionWarnings(workspacePath: string, stats: Array<{ file: string; lines: number | null; mtime_ms: number | null }>): string[] {
  const budgets: Record<string, number> = {
    '.octocode/AGENTS.md': 80,
    '.octocode/MEMORY.md': 200,
    '.octocode/GOTCHAS.md': 200,
    '.octocode/LEARN.md': 200,
    '.octocode/BOOKMARKS.md': 200,
  };
  const markdownWarnings = stats.flatMap(stat => {
    const budget = budgets[stat.file];
    if (stat.lines == null) return [`${stat.file} missing; run repo inject when projection context is needed`];
    if (budget != null && stat.lines > budget) return [`${stat.file} has ${stat.lines} lines over budget ${budget}`];
    return [];
  });
  return [...markdownWarnings, ...manifestWarnings(workspacePath, stats)];
}

function evidenceTrust(references: string[]): AttendEvidence['trust'] {
  if (references.length === 0) return 'needs_refs';
  if (references.some(ref => ref.includes('.octocode/') || ref.startsWith('http'))) return 'generated_or_external_lead';
  return 'verified_lead';
}

function resourceLeads(query: string, workspacePath: string): Array<Record<string, string>> {
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

function chooseMode(query: string, evidenceCount: number, verifyCount: number, gapCount: number): 'explore' | 'exploit' | 'mixed' {
  if (verifyCount > 0 && gapCount === 0) return 'exploit';
  if (evidenceCount === 0 || /(design|rfc|brainstorm|research|unknown|approach|why|how)/i.test(query)) return verifyCount > 0 ? 'mixed' : 'explore';
  return gapCount > 0 ? 'mixed' : 'exploit';
}

export function attendAwareness(db: DatabaseSync, params: AttendParams = {}): AttendResult {
  const cwd = params.cwd ? resolve(params.cwd) : process.cwd();
  const workspacePath = resolve(String(params.workspacePath ?? params.workspace_path ?? params.workspace ?? cwd));
  const limit = limitOf(params.limit);
  const query = String(params.query ?? '').trim();
  const files = stringList(params.file);
  const includeBodies = Boolean(params.includeBodies ?? params.include_bodies);
  const explainOrgan = Boolean(params.explainOrgan ?? params.explain_organ);
  const compact = Boolean(params.compact);
  const packetLimit = compact ? 1 : limit;
  const scope = {
    workspacePath,
    artifact: params.artifact ?? null,
    repo: params.repo ?? null,
    ref: params.ref ?? null,
    query: query || null,
    limit,
    includeBodies,
    cwd,
  };

  const profileResult = queryAwareness(db, { ...scope, view: 'repo-profile' });
  const profile = profileMap(profileResult.rows);
  const workboardResult = queryAwareness(db, { ...scope, view: 'workboard', query: null });
  const rawWorkboard = groupWorkboard(workboardResult.rows);
  const handoffRows = (rawWorkboard['Inbox'] ?? [])
    .filter(row => row['item_type'] === 'refinement' && row['quality'] === 'handoff')
    .slice(0, packetLimit)
    .map(row => compact ? compactRow(row) : row);
  const workboard = compact ? compactWorkboard(rawWorkboard, packetLimit) : rawWorkboard;
  const verificationTargets = (rawWorkboard['Verify'] ?? []).slice(0, packetLimit).map(row => compact ? compactVerificationTarget(row) : row);
  const projectionHealth = projectionStats(workspacePath);
  const bloatWarnings = projectionWarnings(workspacePath, projectionHealth);
  const outputBloatWarnings = compact
    ? bloatWarnings.map(warning => warning
      .replace(/\.octocode\//g, '')
      .replace(/ has /g, ' ')
      .replace(/ lines over budget /g, '>')
      .replace(/ lines/g, 'l'))
    : bloatWarnings;

  const memoryQuery = query || files.join(' ');
  const recall = memoryQuery
    ? getMemory(db, {
      query: memoryQuery,
      limit: Math.min(5, limit),
      minImportance: 1,
      smart: true,
      workspacePath,
      artifact: params.artifact ?? null,
      repo: params.repo ?? null,
      ref: params.ref ?? null,
      files,
      explain: true,
      cwd,
    })
    : { count: 0, memories: [], mode: 'lexical' as const, sort: 'smart', as_of: null, global_only: false, states: ['ACTIVE'] };

  const evidence: AttendEvidence[] = recall.memories.slice(0, packetLimit).map(memory => {
    const allReferences = memory.references ?? [];
    const references = compact ? allReferences.slice(0, 3) : allReferences;
    const why = [
      query ? `matched query "${summarize(query, 80)}"` : null,
      files.length > 0 ? `scoped to ${files.join(', ')}` : null,
      `importance ${memory.importance}`,
      memory.failure_signature ? 'has failure signature' : null,
    ].filter((item): item is string => Boolean(item));
    return {
      kind: 'memory',
      id: memory.memory_id,
      label: memory.label,
      importance: memory.importance,
      title: summarize(memory.task_context, compact ? 90 : 120),
      summary: summarize(memory.observation, compact ? 160 : 240),
      references,
      reference_count: allReferences.length,
      omitted_reference_count: Math.max(0, allReferences.length - references.length),
      why_selected: why,
      trust: evidenceTrust(allReferences),
    };
  });

  const trustWarnings = evidence
    .filter(item => item.trust !== 'verified_lead')
    .map(item => `${item.id}: ${item.trust}`);
  const gaps = [
    query ? null : 'No query supplied; packet is a general workspace briefing.',
    evidence.length === 0 && memoryQuery ? `No memory evidence selected for "${summarize(memoryQuery, 80)}".` : null,
    verificationTargets.length === 0 ? null : `${verificationTargets.length} verification target(s) need attention.`,
    bloatWarnings.length === 0 ? null : `${bloatWarnings.length} projection/bloat warning(s) present.`,
  ].filter((gap): gap is string => Boolean(gap));

  const mode = chooseMode(query, evidence.length, verificationTargets.length, gaps.length);
  const resourceLeadRows = resourceLeads(query || memoryQuery, workspacePath)
    .slice(0, compact ? 2 : limit)
    .map(lead => {
      const source = lead.source ?? '';
      return compact && source.startsWith(`${workspacePath}/`)
        ? { ...lead, source: source.slice(workspacePath.length + 1) }
        : lead;
    });
  const alternatives = mode === 'explore' || mode === 'mixed'
    ? [
      { option: 'derive_view_first', why: 'Prefer read-only DB projections before new canonical storage.' },
      { option: 'narrow_scope', why: 'Use query/file filters if the packet is too broad.' },
    ]
    : [];

  const compactProjectionHealth = compact
    ? projectionHealth.map(item => ({ file: item.file, lines: item.lines }))
    : projectionHealth;
  const organState = {
    senses: {
      ...(compact ? {} : { profile }),
      projection_health: compactProjectionHealth,
    },
    attention: {
      selected_evidence: evidence.length,
      workboard_items: workboardResult.count,
      compact_budget: compact ? '<=8KB JSON' : 'unbounded caller output',
    },
    memory: {
      active_memories: profile['active_memories'] ?? 0,
      gotchas: profile['gotchas'] ?? 0,
      lessons: profile['lessons'] ?? 0,
      recall_mode: recall.mode,
    },
    error_signals: {
      verification_targets: verificationTargets.length,
      trust_warnings: trustWarnings.length,
    },
    pruning_candidates: {
      memory_review: workboard['MemoryReview']?.length ?? 0,
      projection_warnings: bloatWarnings.length,
    },
    bridge: {
      inbox: workboard['Inbox']?.length ?? 0,
      handoffs: handoffRows.length,
      open_refinements: profile['open_refinements'] ?? 0,
      open_signals: profile['open_signals'] ?? 0,
    },
    projection: {
      warnings: outputBloatWarnings,
    },
  };

  const signalIds = uniqueStrings((workboard['Inbox'] ?? []).filter(row => row['item_type'] === 'signal').map(row => String(row['id'])));
  const handoffIds = uniqueStrings(handoffRows.map(row => String(row['id'])));
  const refinementIds = uniqueStrings(Object.values(workboard).flat().filter(row => row['item_type'] === 'refinement').map(row => String(row['id'])));
  const agentIds = uniqueStrings(Object.values(workboard).flat().map(row => String(row['agent_id'] ?? '')));
  const sourceRefs = evidence.flatMap(item => item.references);
  const driveState = {
    goal: query || 'general workspace awareness',
    mode,
    learning_gaps: gaps,
    resource_leads: resourceLeadRows,
    alternatives,
    team_norms: TEAM_NORMS,
    transactive_map: {
      memory_ids: evidence.map(item => item.id),
      signal_ids: signalIds.slice(0, compact ? 3 : 12),
      signal_id_count: signalIds.length,
      handoff_ids: handoffIds.slice(0, compact ? 3 : 12),
      handoff_id_count: handoffIds.length,
      refinement_ids: refinementIds.slice(0, compact ? 4 : 12),
      refinement_id_count: refinementIds.length,
      agent_ids: agentIds.slice(0, compact ? 6 : 24),
      agent_id_count: agentIds.length,
      source_refs: sourceRefs.slice(0, compact ? 5 : 12),
      source_ref_count: sourceRefs.length,
    },
  };

  const result: AttendResult = {
    ok: true,
    schema_version: 1,
    generated_at: profileResult.generated_at,
    workspace_path: workspacePath,
    artifact: params.artifact ?? null,
    repo: params.repo ?? null,
    ref: params.ref ?? null,
    profile,
    organ_state: organState,
    drive_state: driveState,
    workboard,
    evidence,
    gaps,
    bloat_warnings: outputBloatWarnings,
    verification_targets: verificationTargets,
    trust_warnings: trustWarnings,
    trace: [
      { step: 'repo-profile', count: profileResult.count },
      { step: 'workboard', count: workboardResult.count },
      { step: 'memory-recall', count: evidence.length, note: memoryQuery ? undefined : 'skipped-empty-query' },
      { step: 'projection-health', count: projectionHealth.length },
    ],
    next: verificationTargets.length > 0
      ? 'octocode-awareness verify audit --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact; then verify mark --all-pending after the declared test plan'
      : bloatWarnings.length > 0
        ? 'octocode-awareness memory forget --workspace "$PWD" --dry-run --compact; then repo inject --workspace "$PWD" --compact to regenerate capped projections (digest does not shrink markdown)'
        : evidence.length > 0
          ? 'Treat evidence as leads; re-check cited files, then lock acquire before edits'
          : 'octocode-awareness attend --workspace "$PWD" --query "<narrower task>" --compact; or query workboard / workspace status',
  };
  if (explainOrgan) result.organ_reference = ORGAN_REFERENCE;
  return result;
}
