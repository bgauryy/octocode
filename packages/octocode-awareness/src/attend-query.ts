import { resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { getMemory } from './memory.js';
import { projectionSourceRevision, queryAwareness } from './repo-context.js';
import { AttendEvidence, AttendParams, AttendResult, chooseMode, compactRow, compactWorkboard, evidenceTrust, groupWorkboard, limitOf, ORGAN_REFERENCE, profileMap, projectionStats, projectionWarnings, resourceLeads, shellQuote, stringList, summarize, TEAM_NORMS, uniqueStrings } from './attend-model.js';

export function attendAwareness(db: DatabaseSync, params: AttendParams = {}): AttendResult {
  const cwd = params.cwd ? resolve(params.cwd) : process.cwd();
  // D1 fix lives in repo-context `scopeFromParams`/`workspaceAliases`: the raw
  // workspace path below flows through to the alias set, which now also matches
  // the git-root key that write paths store — so the profile block does not
  // undercount rows written from a package/subdir.
  const workspacePath = resolve(String(params.workspacePath ?? params.workspace_path ?? params.workspace ?? cwd));
  const limit = limitOf(params.limit);
  const query = String(params.query ?? '').trim();
  const files = stringList(params.file);
  const includeBodies = Boolean(params.includeBodies ?? params.include_bodies);
  const explainOrgan = Boolean(params.explainOrgan ?? params.explain_organ);
  const compact = Boolean(params.compact);
  const agentId = String(params.agentId ?? params.agent_id ?? '').trim();
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
  if (agentId && rawWorkboard['Verify']) {
    rawWorkboard['Verify'] = [...rawWorkboard['Verify']!].sort((left, right) =>
      Number(String(right['agent_id'] ?? '') === agentId) - Number(String(left['agent_id'] ?? '') === agentId));
  }
  const handoffRows = (rawWorkboard['Inbox'] ?? [])
    .filter(row => row['item_type'] === 'refinement' && row['quality'] === 'handoff')
    .slice(0, packetLimit)
    .map(row => compact ? compactRow(row) : row);
  const workboard = compact ? compactWorkboard(rawWorkboard, packetLimit) : rawWorkboard;
  const verificationTargets = (rawWorkboard['Verify'] ?? [])
    .filter(row => agentId !== '' && String(row['agent_id'] ?? '') === agentId)
    .slice(0, packetLimit);
  const readyTasks = rawWorkboard['Ready'] ?? [];
  const claimedTasks = (rawWorkboard['Claimed'] ?? []).filter(row => row['item_type'] === 'task');
  const projectionHealth = projectionStats(workspacePath);
  const bloatWarnings = projectionWarnings(workspacePath, projectionHealth, () => projectionSourceRevision(db, {
    workspacePath,
    artifact: scope.artifact,
    repo: scope.repo,
    ref: scope.ref,
    limit: 500,
  }));
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
      projection_warnings: bloatWarnings.length,
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
    .map(row => String(row['path'] ?? row['file_path'] ?? ''))
    .find(path => path.length > 0);
  const inboxCount = (rawWorkboard['Inbox'] ?? []).length > 0
    ? Number((rawWorkboard['Inbox'] ?? [])[0]?.['column_total'] ?? (rawWorkboard['Inbox'] ?? []).length)
    : 0;

  const workspaceArg = shellQuote(workspacePath);
  const agentArg = agentId ? shellQuote(agentId) : '"$OCTOCODE_AGENT_ID"';

  const next = verificationTargets.length > 0
    ? `octocode-awareness verify audit --agent-id ${agentArg} --workspace ${workspaceArg} --compact${verificationRunId ? `; after its declared test plan: octocode-awareness verify mark --run-id ${shellQuote(verificationRunId)} --agent-id ${agentArg} --message "<check + result>" --compact` : ''}`
    : readyTasks.length > 0
      ? `octocode-awareness task claim --task-id ${shellQuote(String(readyTasks[0]?.['id']))} --agent-id ${agentArg} --compact`
      : ownedClaimedTask && ownedClaimedRunId
        ? `octocode-awareness task heartbeat --task-id ${shellQuote(String(ownedClaimedTask['id']))} --run-id ${shellQuote(ownedClaimedRunId!)} --agent-id ${agentArg} --compact`
        : ownedClaimedTask
          ? `octocode-awareness task show --task-id ${shellQuote(String(ownedClaimedTask['id']))} --compact`
        : filesUnderWorkPath
          ? `octocode-awareness work show --workspace ${workspaceArg} --file ${shellQuote(filesUnderWorkPath)} --compact; read peer reason before overlapping edits`
        : inboxCount > 0
          ? `octocode-awareness signal list --agent-id ${agentArg} --workspace ${workspaceArg} --limit 3 --compact`
          : !query && bloatWarnings.length > 0
            ? `octocode-awareness query workboard --workspace ${workspaceArg} --format json --limit 5 --compact`
            : evidence.length > 0
              ? 'Treat evidence as leads; re-check cited files, then work start before edits'
              : `octocode-awareness attend --workspace ${workspaceArg} --agent-id ${agentArg} --query "<narrower task>" --compact`;

  if (compact) {
    const columnCount = (column: string): number => {
      const rows = rawWorkboard[column] ?? [];
      return Number(rows[0]?.['column_total'] ?? rows.length);
    };
    return {
      ok: true,
      generated_at: profileResult.generated_at,
      workspace_path: workspacePath,
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
    next,
  };
  if (explainOrgan) result.organ_reference = ORGAN_REFERENCE;
  return result;
}
