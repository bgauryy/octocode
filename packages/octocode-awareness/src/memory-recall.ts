import type { DatabaseSync } from 'node:sqlite';
import { normalizeReferences, normalizeLabel } from './helpers.js';
import { hasFts } from './db.js';
import type { GetMemoryParams, GetMemoryResult } from './types.js';
import { anyReferenceCandidateIds, attachMemoryReferences, compileRecallRegex, exactReferenceCandidateIds, fileReferenceCandidates, fileRegexCandidateIds, intersectCandidateIds, lexicalSearch, regexCandidateIds } from './memory-search.js';
import { bumpAccess } from './memory-write.js';
import { canonicalMemoryInstant, decayComponents, JUDGMENT_RELEVANCE_FLOOR, LexicalScopeOptions, SCORING_PREFETCH_FACTOR } from './memory-scoring.js';

// ─── getMemory ────────────────────────────────────────────────────────────────

/**
 * Recall memories using FTS5 + decay scoring.
 */
export function getMemory(db: DatabaseSync, params: GetMemoryParams = {}): GetMemoryResult {
  const {
    query = '',
    limit: limitRaw = 3,
    minImportance: minImpRaw = 1,
    label,
    tags = [],
    smart = false,
    workspacePath,
    artifact,
    repo: repoArg,
    ref: refArg,
    states: statesRaw,
    sort = 'smart',
    globalOnly = false,
    strictScope = false,
    asOf,
    references = [],
    regex = [],
    fileRegex = [],
    files = [],
    explain = false,
    candidateMemoryIds = [],
    recordAccess = true,
    cwd: cwdParam,
  } = params;

  // Alternate rankers may pass the bounded 2,000-row embedding pool through
  // this function so every normal scope/provenance filter runs before top-k.
  const limitCap = candidateMemoryIds.length > 0 ? 2_000 : 50;
  const limit = Math.min(limitCap, Math.max(1, Number(limitRaw) || 3));
  const smartEnabled = smart === true || smart === 'true';
  let minImportance = Math.max(1, Number(minImpRaw) || 1);
  let smartExpanded = false;
  const droppedSmartFilters: string[] = [];

  const states = statesRaw ?? (asOf ? ['ACTIVE', 'SUPERSEDED'] : ['ACTIVE']);
  const labels = label
    ? (Array.isArray(label) ? label.map((value) => normalizeLabel(value)) : [normalizeLabel(label)])
    : [];
  let appliedMinImportance = minImportance;
  let appliedLabels = [...labels];
  let appliedTags = [...tags];

  const effectiveCwd = cwdParam ?? workspacePath ?? undefined;
  const normalizedAsOf = canonicalMemoryInstant(asOf, 'as_of');
  const asOfDate = normalizedAsOf ? new Date(normalizedAsOf) : null;
  if (asOfDate && isNaN(asOfDate.getTime())) {
    throw new Error(`invalid --as-of value "${asOf}" — expected ISO 8601 date string (e.g. 2024-06-01T00:00:00Z)`);
  }

  let candidateIds: Set<string> | null = candidateMemoryIds.length > 0
    ? new Set(candidateMemoryIds.filter(Boolean))
    : null;
  const refFilters = normalizeReferences(references);
  const fileRefFilters = fileReferenceCandidates(files, effectiveCwd);
  const compiledRegex = regex.map(compileRecallRegex);
  const compiledFileRegex = fileRegex.map(compileRecallRegex);

  if (refFilters.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, exactReferenceCandidateIds(db, refFilters));
  }
  if (fileRefFilters.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, anyReferenceCandidateIds(db, fileRefFilters));
  }
  if (compiledFileRegex.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, fileRegexCandidateIds(db, compiledFileRegex));
  }
  if (compiledRegex.length > 0) {
    candidateIds = intersectCandidateIds(candidateIds, regexCandidateIds(db, compiledRegex));
  }

  const scopeOptions: LexicalScopeOptions = {
    workspacePath: workspacePath ?? cwdParam ?? null,
    artifact,
    repo: repoArg,
    ref: refArg,
    strictScope,
    globalOnly,
    cwd: cwdParam,
    asOf: normalizedAsOf,
    candidateMemoryIds: candidateIds ? [...candidateIds] : undefined,
  };
  let memories = lexicalSearch(
    db, query, limit * SCORING_PREFETCH_FACTOR, minImportance, tags, labels, states, {
      ...scopeOptions,
    },
  );
  if (smartEnabled && memories.length < limit && (labels.length > 0 || tags.length > 0 || minImportance > 1)) {
    if (labels.length > 0 && !droppedSmartFilters.includes('label')) droppedSmartFilters.push('label');
    if (tags.length > 0 && !droppedSmartFilters.includes('tag')) droppedSmartFilters.push('tag');
    if (minImportance > 1 && !droppedSmartFilters.includes('min_importance')) droppedSmartFilters.push('min_importance');
    const expanded = lexicalSearch(
      db,
      query,
      limit * SCORING_PREFETCH_FACTOR,
      1,
      [],
      [],
      states,
      scopeOptions,
    );
    const byId = new Map(memories.map(memory => [memory.memory_id, memory]));
    for (const memory of expanded) byId.set(memory.memory_id, memory);
    memories = [...byId.values()];
    smartExpanded = true;
    appliedMinImportance = 1;
    appliedLabels = [];
    appliedTags = [];
  }
  attachMemoryReferences(db, memories);

  // Exact file filter — the `file` column was removed from the schema (files are
  // now tracked via memory_refs with prefix "file:"). For forward compatibility we
  // keep the filter logic but match against the references array instead.
  if (fileRefFilters.length > 0) {
    const normFiles = new Set(fileRefFilters);
    memories = memories.filter(m =>
      m.references.some(r => normFiles.has(r))
    );
  }

  // Reference filters are conjunctive: every provided provenance reference must
  // be present. This is pushed down before the search cap and rechecked here.
  if (refFilters.length > 0) {
    memories = memories.filter(m => refFilters.every(ref => m.references.includes(ref)));
  }

  // Regex filter
  if (compiledRegex.length > 0 || compiledFileRegex.length > 0) {
    memories = memories.filter(m => {
      if (compiledFileRegex.length > 0) {
        // Match fileRegex against file: prefixed references (no standalone file column)
        const fileRefs = (m.references ?? []).filter(r => r.startsWith('file:'));
        if (!compiledFileRegex.every(re => fileRefs.some(r => re.test(r)))) return false;
      }
      if (compiledRegex.length > 0) {
        const haystack = [
          m.task_context, m.observation,
          ...(m.tags ?? []), ...(m.references ?? []),
          m.label, m.workspace_path, m.artifact, m.repo, m.ref, m.failure_signature,
        ].filter(Boolean).join(' ');
        if (!compiledRegex.every(re => re.test(haystack))) return false;
      }
      return true;
    });
  }

  if (asOfDate) {
    memories = memories.filter(m => {
      const vf = m.valid_from ? new Date(m.valid_from) : null;
      const vt = m.valid_to ? new Date(m.valid_to) : null;
      return (!vf || vf <= asOfDate) && (!vt || vt > asOfDate);
    });
  }

  // Sort
  const stableTieBreak = (a: (typeof memories)[number], b: (typeof memories)[number]) =>
    (a.memory_id ?? '').localeCompare(b.memory_id ?? '');
  if (sort === 'importance') {
    memories.sort((a, b) =>
      (b.importance - a.importance) || ((b.score ?? 0) - (a.score ?? 0)) || stableTieBreak(a, b));
  } else if (sort === 'recent') {
    memories.sort((a, b) => (b.created_at ?? '').localeCompare(a.created_at ?? '') || stableTieBreak(a, b));
  } else if (sort === 'accessed') {
    memories.sort((a, b) =>
      (b.last_accessed_at ?? b.created_at ?? '').localeCompare(a.last_accessed_at ?? a.created_at ?? '') || stableTieBreak(a, b));
  } else {
    memories.sort((a, b) => ((b.score ?? 0) - (a.score ?? 0)) || stableTieBreak(a, b));
  }

  memories = memories.slice(0, limit);
  if (explain) {
    for (const m of memories) {
      const components = decayComponents(m, m.lexical ?? 0);
      m.score_components = components;
      m.score = components.final;
    }
  }
  if (recordAccess) bumpAccess(db, memories.map(m => m.memory_id));

  const mode = hasFts(db) ? 'lexical' as const : 'fallback' as const;
  const result: GetMemoryResult & {
    smart_expanded?: boolean;
    smart_dropped_filters?: string[];
  } = {
    count: memories.length,
    memories,
    mode,
    sort,
    as_of: normalizedAsOf,
    global_only: Boolean(globalOnly),
    states,
    ...(explain ? {
      applied_filters: {
        query,
        limit,
        min_importance: appliedMinImportance,
        labels: appliedLabels,
        tags: appliedTags,
        references: refFilters,
        files,
        file_regex: fileRegex,
        regex,
        workspace_path: workspacePath ?? cwdParam ?? null,
        artifact: artifact ?? null,
        repo: repoArg ?? null,
        ref: refArg ?? null,
        strict_scope: Boolean(strictScope),
        global_only: Boolean(globalOnly),
        states,
        as_of: normalizedAsOf,
        sort,
        smart: smartEnabled,
      },
    } : {}),
    ...(smartExpanded ? {
      smart_expanded: true,
      smart_dropped_filters: droppedSmartFilters,
    } : {}),
  };

  // Low-confidence recall flag (engram BM25Floor pattern): tell the agent when
  // results are weak matches so it verifies before relying on them.
  if (query.trim()) {
    const topRelevance = memories[0]?.lexical ?? 0;
    if (memories.length === 0) {
      result.judgment_required = true;
      result.judgment_reason = smartEnabled
        ? 'no results after smart widening — absence of recall is not proof of absence; broaden the query terms or scope'
        : 'no results — absence of recall is not proof of absence; retry with --smart or broader terms';
    } else if (mode === 'fallback') {
      result.judgment_required = true;
      result.judgment_reason = 'FTS unavailable — results are unranked substring matches; verify relevance before relying on them';
    } else if (topRelevance < JUDGMENT_RELEVANCE_FLOOR) {
      result.judgment_required = true;
      result.judgment_reason = `weak lexical match (top relevance ${topRelevance.toFixed(2)} < ${JUDGMENT_RELEVANCE_FLOOR}) — treat results as leads, not answers`;
    }
  }

  return result;
}
