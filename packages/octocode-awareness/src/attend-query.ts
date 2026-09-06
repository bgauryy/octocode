import { decideNext } from './attend-flow.js';
import { getDatabasePath } from './db-runtime.js';
import { resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { assertKnownOptions } from './helpers.js';
import { getMemory } from './memory-recall.js';
import { queryAwareness } from './repo-query.js';
import { AttendEvidence, AttendParams, AttendResult, chooseMode, compactRow, compactWorkboard, evidenceTrust, groupWorkboard, limitOf, ORGAN_REFERENCE, profileMap, resourceLeads, stringList, summarize, TEAM_NORMS, uniqueStrings } from './attend-model.js';
import type { AwarenessQueryRow } from './repo-model.js';
import { assessOperationalState, scopedWorkRows } from './attend-physiology.js';

const ATTEND_OPTION_KEYS = [
  'runtimeObservation', 'agentId', 'workspacePath', 'artifact', 'repo', 'ref', 'query',
  'file', 'limit', 'compact', 'includeBodies', 'explainOrgan', 'cwd',
] as const;

function clusterCompactHandoffs(rows: AwarenessQueryRow[]): AwarenessQueryRow[] {
  const clusters = new Map<string, { row: AwarenessQueryRow; count: number; ids: string[] }>();
  const output: AwarenessQueryRow[] = [];
  for (const row of rows) {
    const isHandoff = row['item_type'] === 'signal' && String(row['title'] ?? '').startsWith('handoff:');
    if (!isHandoff) {
      output.push(row);
      continue;
    }
    const files = Array.isArray(row['files']) ? row['files'].map(String).sort() : [];
    const key = JSON.stringify({
      agent: String(row['agent_id'] ?? ''),
      title: String(row['title'] ?? ''),
      detail: String(row['detail'] ?? '').slice(0, 120),
      files,
    });
    const ids = Array.isArray(row['raw_ids']) ? row['raw_ids'].map(String) : [String(row['id'] ?? '')].filter(Boolean);
    const existing = clusters.get(key);
    if (existing) {
      existing.count += 1;
      existing.ids.push(...ids);
      existing.row['title'] = `handoff cluster (${existing.count}): ${summarize(String(row['title'] ?? 'handoff'), 80)}`;
      existing.row['id'] = String(row['id'] ?? existing.row['id']);
      continue;
    }
    const clustered: AwarenessQueryRow = {
      ...row,
      title: `handoff cluster (1): ${summarize(String(row['title'] ?? 'handoff'), 80)}`,
    };
    clusters.set(key, { row: clustered, count: 1, ids: [...ids] });
    output.push(clustered);
  }
  for (const cluster of clusters.values()) {
    cluster.row['raw_ids'] = uniqueStrings(cluster.ids);
  }
  return output;
}

export function attendAwareness(db: DatabaseSync, params: AttendParams = {}): AttendResult {
  assertKnownOptions(params, ATTEND_OPTION_KEYS, 'attendAwareness');
  const cwd = params.cwd ? resolve(params.cwd) : process.cwd();
  // D1 fix lives in repo-query `scopeFromParams`/`workspaceAliases`: the raw
  // workspace path below flows through to the normalized path set, which also matches
  // the git-root key that write paths store — so the profile block does not
  // undercount rows written from a package/subdir.
  const workspacePath = resolve(String(params.workspacePath ?? cwd));
  const limit = limitOf(params.limit);
  const query = String(params.query ?? '').trim();
  const files = stringList(params.file);
  const includeBodies = Boolean(params.includeBodies);
  const explainOrgan = Boolean(params.explainOrgan);
  const compact = Boolean(params.compact);
  const agentId = String(params.agentId ?? '').trim();
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
  const workboardResult = queryAwareness(db, { ...scope, view: 'workboard', query: null, preferAgentId: agentId || null, preferFiles: files });
  const rawWorkboard = groupWorkboard(workboardResult.rows);
  if (agentId && rawWorkboard['Verify']) {
    rawWorkboard['Verify'] = [...rawWorkboard['Verify']!].sort((left, right) =>
      Number(String(right['agent_id'] ?? '') === agentId) - Number(String(left['agent_id'] ?? '') === agentId));
  }
  const compactSourceWorkboard = compact
    ? { ...rawWorkboard, Inbox: clusterCompactHandoffs(rawWorkboard['Inbox'] ?? []) }
    : rawWorkboard;
  const handoffRows = (compactSourceWorkboard['Inbox'] ?? [])
    .filter(row => row['item_type'] === 'signal' && String(row['title'] ?? '').startsWith('handoff'))
    .slice(0, packetLimit)
    .map(row => compact ? compactRow(row) : row);
  const workboard = compact ? compactWorkboard(compactSourceWorkboard, packetLimit) : rawWorkboard;
  const verificationTargets = (rawWorkboard['Verify'] ?? [])
    .filter(row => agentId !== '' && String(row['agent_id'] ?? '') === agentId)
    .slice(0, packetLimit);
  const readyTasks = rawWorkboard['Ready'] ?? [];
  const claimedTasks = (rawWorkboard['Claimed'] ?? []).filter(row => row['item_type'] === 'task');

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
      recordAccess: false,
      cwd,
    })
    : { count: 0, memories: [], mode: 'lexical' as const, sort: 'smart', as_of: null, global_only: false, states: ['ACTIVE'] };

  const evidence: AttendEvidence[] = recall.memories.slice(0, packetLimit).map(memory => {
    const allReferences = memory.references ?? [];
    const references = compact ? allReferences.slice(0, 1) : allReferences;
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
      title: summarize(memory.task_context, compact ? 60 : 120),
      summary: summarize(memory.observation, compact ? 120 : 240),
      references,
      ...(compact ? {} : {
        reference_count: allReferences.length,
        omitted_reference_count: Math.max(0, allReferences.length - references.length),
      }),
      why_selected: compact ? why.slice(0, 2) : why,
      trust: evidenceTrust(allReferences, workspacePath),
    };
  });

  const trustWarnings = evidence
    .filter(item => item.trust !== 'existing_file_lead')
    .map(item => `${item.id}: ${item.trust}`);
  const physiology = assessOperationalState({
    workboard: rawWorkboard,
    ...(params.runtimeObservation === undefined ? {} : { runtimeObservation: params.runtimeObservation }),
    agentId,
    workspacePath,
    files,
    recalled: recall.memories.length,
    referenceWarnings: recall.memories.filter(memory => evidenceTrust(memory.references ?? [], workspacePath) !== 'existing_file_lead').length,
  });
  const gaps = [
    query ? null : 'No query supplied; packet is a general workspace briefing.',
    evidence.length === 0 && memoryQuery ? `No memory evidence selected for "${summarize(memoryQuery, 80)}".` : null,
    verificationTargets.length === 0 ? null : `${verificationTargets.length} verification target(s) need attention.`,
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
      { option: 'derive_view_first', why: 'Prefer read-only DB views before new canonical storage.' },
      { option: 'narrow_scope', why: 'Use query/file filters if the packet is too broad.' },
    ]
    : [];

  const organState = {
    senses: {
      ...(compact ? {} : { profile }),
    },
    attention: {
      selected_evidence: evidence.length,
      workboard_items: workboardResult.count,
      ready_tasks: readyTasks.length,
      claimed_tasks: claimedTasks.length,
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
    },
    bridge: {
      inbox: workboard['Inbox']?.length ?? 0,
      handoffs: handoffRows.length,
      actionable_refinements: profile['actionable_refinements'] ?? 0,
      all_open_refinements: profile['all_open_refinements'] ?? 0,
      open_signals: profile['open_signals'] ?? 0,
      plans: profile['plans'] ?? 0,
      tasks: profile['tasks'] ?? 0,
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
      ready_task_ids: readyTasks.map(row => String(row['id'])).slice(0, compact ? 3 : 12),
      claimed_task_ids: claimedTasks.map(row => String(row['id'])).slice(0, compact ? 3 : 12),
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

  const verificationRunId = verificationTargets
    .flatMap(row => Array.isArray(row['raw_ids']) ? row['raw_ids'] as unknown[] : [])
    .map(String)
    .find(id => id.startsWith('run_'));
  const ownedClaimed = agentId
    ? claimedTasks.filter(row => String(row['agent_id'] ?? '') === agentId)
    : [];
  const ownedClaimedTask = ownedClaimed[0];
  const ownedClaimedRunId = ownedClaimed
    .flatMap(row => Array.isArray(row['raw_ids']) ? row['raw_ids'] as unknown[] : [])
    .map(String)
    .find(id => id.startsWith('run_'));
  const filesUnderWork = rawWorkboard['FilesUnderWork'] ?? [];
  const filesUnderWorkPath = filesUnderWork
    .filter(row => !agentId || (Array.isArray(row['agents']) && row['agents'].some(agent => String(agent) !== agentId)))
    .map(row => String(row['path'] ?? row['file_path'] ?? ''))
    .find(path => path.length > 0);
  const inboxCount = (rawWorkboard['Inbox'] ?? []).length > 0
    ? Number((rawWorkboard['Inbox'] ?? [])[0]?.['column_total'] ?? (rawWorkboard['Inbox'] ?? []).length)
    : 0;

  const scopedWork = scopedWorkRows(rawWorkboard, workspacePath, files);
  const scopedInspection = scopedWork.find(row => row['locked'] === true && String(row['lock_agent'] ?? '') !== agentId)
    ?? scopedWork.find(row => Array.isArray(row['agents']) && (!agentId || row['agents'].some(agent => String(agent) !== agentId)));
  const scopedInspectionPath = scopedInspection ? String(scopedInspection['path'] ?? scopedInspection['file_path']) : null;

  const next = decideNext({
    databasePath: getDatabasePath(db), workspacePath, artifact: params.artifact, agentId,
    verificationRequired: verificationTargets.length > 0, verificationRunId,
    ...(scopedInspectionPath ? { inspection: {
      file: scopedInspectionPath,
      locked: scopedInspection?.['locked'] === true && String(scopedInspection['lock_agent'] ?? '') !== agentId,
    } } : {}),
    runtimeActions: physiology.regulation.actions,
    ownedTaskId: ownedClaimedTask ? String(ownedClaimedTask['id']) : undefined,
    ownedRunId: ownedClaimedRunId,
    peerFile: files.length === 0 ? filesUnderWorkPath : undefined,
    inboxCount, hasEvidence: evidence.length > 0,
    readyTaskId: readyTasks.length > 0 && !query ? String(readyTasks[0]?.['id']) : undefined,
  });

  if (compact) {
    const columnCount = (column: string): number => {
      const rows = rawWorkboard[column] ?? [];
      return Number(rows[0]?.['column_total'] ?? rows.length);
    };
    return {
      ok: true,
      generated_at: profileResult.generated_at,
      workspace_path: workspacePath,
      ...physiology,
      ...(explainOrgan ? { organ_reference: ORGAN_REFERENCE } : {}),
      ...(params.artifact ? { artifact: params.artifact } : {}),
      ...(params.repo ? { repo: params.repo } : {}),
      ...(params.ref ? { ref: params.ref } : {}),
      counts: {
        Inbox: columnCount('Inbox'),
        Ready: columnCount('Ready'),
        Claimed: columnCount('Claimed'),
        Verify: columnCount('Verify'),
        FilesUnderWork: columnCount('FilesUnderWork'),
        Maintenance: columnCount('Maintenance'),
      },
      workboard,
      evidence,
      next,
    };
  }

  const result: AttendResult = {
    ok: true,
    generated_at: profileResult.generated_at,
    workspace_path: workspacePath,
    ...physiology,
    artifact: params.artifact ?? null,
    repo: params.repo ?? null,
    ref: params.ref ?? null,
    profile,
    organ_state: organState,
    drive_state: driveState,
    workboard,
    evidence,
    gaps,
    verification_targets: verificationTargets,
    trust_warnings: trustWarnings,
    trace: [
      { step: 'repo-profile', count: profileResult.count },
      { step: 'workboard', count: workboardResult.count },
      { step: 'memory-recall', count: evidence.length, note: memoryQuery ? undefined : 'skipped-empty-query' },
    ],
    next,
  };
  if (explainOrgan) result.organ_reference = ORGAN_REFERENCE;
  return result;
}
