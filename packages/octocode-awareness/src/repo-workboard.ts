import type { DatabaseSync } from 'node:sqlite';
import { inspectMaintenancePressure } from './maintenance.js';
import { AwarenessQueryParams, AwarenessQueryRow, limitOf, utcNow } from './repo-model.js';
import { filesUnderWorkRows, pushLimited, repoProfileRows, rowFiles } from './repo-files.js';
import { scopeFromParams, withScope } from './repo-scope.js';
import { countPendingStandaloneRuns, developerReviewRows, refinementRows, signalRows } from './repo-coordination.js';
import { summarize } from './repo-formats.js';
import { countTaskRows, memoryRows, runRows, taskRows } from './repo-plans.js';

export function workboardRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const limit = limitOf(params.limit, 10, 50);
  const columns: Record<string, AwarenessQueryRow[]> = {
    Inbox: [],
    Verify: [],
    Ready: [],
    Claimed: [],
    FilesUnderWork: [],
    RecentDone: [],
    MemoryReview: [],
    DeveloperReview: [],
    ProjectionHealth: [],
    Maintenance: [],
  };
  const counts: Record<string, number> = {};

  for (const row of filesUnderWorkRows(db, withScope(params, { limit: 200 }))) {
    pushLimited(columns, counts, 'FilesUnderWork', row, limit);
  }

  const openSignals = signalRows(db, withScope(params, { state: ['open'], limit: 200, includeBodies: false }));
  for (const row of openSignals) {
    pushLimited(columns, counts, 'Inbox', {
      item_type: 'signal',
      id: String(row['signal_id']),
      title: `${row['kind']}: ${summarize(String(row['subject']), 100)}`,
      detail: summarize(String(row['body'] ?? ''), 180),
      agent_id: String(row['from_agent']),
      status: String(row['status']),
      raw_ids: [String(row['signal_id'])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
    }, limit);
  }

  const handoffs = refinementRows(db, withScope(params, { state: ['open', 'ongoing'], limit: 200 }))
    .filter(row => String(row['quality']) === 'handoff');
  for (const row of handoffs) {
    pushLimited(columns, counts, 'Inbox', {
      item_type: 'refinement',
      id: String(row['refinement_id']),
      title: summarize(String(row['remember']), 100),
      detail: summarize(String(row['reasoning']), 180),
      agent_id: String(row['agent_id']),
      status: String(row['state']),
      quality: String(row['quality']),
      raw_ids: [String(row['refinement_id'])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
      updated_at: String(row['updated_at']),
    }, limit);
  }

  for (const row of taskRows(db, withScope(params, { state: ['VERIFY'], limit: 500 }))) {
    pushLimited(columns, counts, 'Verify', {
      item_type: 'task',
      id: String(row['task_id']),
      title: summarize(String(row['title']), 120),
      detail: summarize(String(row['acceptance_criteria']), 180),
      plan_id: String(row['plan_id']),
      agent_id: String(row['claimed_by'] ?? row['created_by']),
      status: String(row['status']),
      raw_ids: [String(row['task_id']), ...(row['run_id'] ? [String(row['run_id'])] : [])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
      updated_at: String(row['updated_at']),
    }, limit);
  }

  // Quick lock-only flows have no plan task; they still owe verification.
  for (const row of runRows(db, withScope(params, { state: ['PENDING'], limit: 500 }))
    .filter(row => row['task_id'] == null)) {
    pushLimited(columns, counts, 'Verify', {
      item_type: 'run',
      id: String(row['run_id']),
      title: summarize(String(row['rationale']), 120),
      detail: summarize(String(row['test_plan']), 180),
      agent_id: String(row['agent_id']),
      status: String(row['status']),
      raw_ids: [String(row['run_id'])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
      updated_at: String(row['updated_at']),
    }, limit);
  }

  for (const row of taskRows(db, withScope(params, { state: ['OPEN'], limit: 500 }))
    .filter(row => row['ready'] === true)) {
    pushLimited(columns, counts, 'Ready', {
      item_type: 'task',
      id: String(row['task_id']),
      title: summarize(String(row['title']), 120),
      detail: summarize(String(row['reasoning']), 180),
      plan_id: String(row['plan_id']),
      agent_id: String(row['created_by']),
      status: String(row['status']),
      priority: Number(row['priority']),
      raw_ids: [String(row['task_id'])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
      updated_at: String(row['updated_at']),
    }, limit);
  }

  for (const row of taskRows(db, withScope(params, { state: ['IN_PROGRESS'], limit: 500 }))) {
    pushLimited(columns, counts, 'Claimed', {
      item_type: 'task',
      id: String(row['task_id']),
      title: summarize(String(row['title']), 120),
      detail: summarize(String(row['reasoning']), 180),
      plan_id: String(row['plan_id']),
      agent_id: String(row['claimed_by']),
      status: String(row['status']),
      raw_ids: [String(row['task_id']), ...(row['run_id'] ? [String(row['run_id'])] : [])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
      updated_at: String(row['updated_at']),
      expires_at: row['claim_expires_at'] ?? null,
    }, limit);
  }

  for (const row of taskRows(db, withScope(params, { state: ['DONE', 'FAILED', 'CANCELLED'], limit: 200 }))) {
    pushLimited(columns, counts, 'RecentDone', {
      item_type: 'task',
      id: String(row['task_id']),
      title: `${row['status']}: ${summarize(String(row['title']), 100)}`,
      detail: summarize(String(row['acceptance_criteria']), 180),
      plan_id: String(row['plan_id']),
      agent_id: String(row['created_by']),
      status: String(row['status']),
      raw_ids: [String(row['task_id'])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
      updated_at: String(row['updated_at']),
    }, limit);
  }

  for (const row of memoryRows(db, withScope(params, { limit: 200 }))) {
    const failureSignature = String(row['failure_signature'] ?? '');
    const refs = Array.isArray(row['references']) ? row['references'] as string[] : [];
    const missingRefs = Array.isArray(row['missing_references']) ? row['missing_references'] as string[] : [];
    const tags = Array.isArray(row['tags']) ? row['tags'] as string[] : [];
    const reviewReasons = [
      refs.length === 0 ? 'missing_refs' : null,
      missingRefs.length > 0 ? 'stale_file_refs' : null,
      failureSignature ? 'failure_signature' : null,
      tags.includes('anti-bloat') ? 'policy_memory' : null,
    ].filter((reason): reason is string => Boolean(reason));
    if (reviewReasons.length === 0) continue;
    pushLimited(columns, counts, 'MemoryReview', {
      item_type: 'memory',
      id: String(row['memory_id']),
      title: `${row['label']}:${row['importance']} ${summarize(String(row['task_context']), 100)}`,
      detail: summarize(String(row['observation']), 180),
      agent_id: String(row['agent_id']),
      status: 'review',
      reasons: reviewReasons,
      missing_reference_count: missingRefs.length,
      missing_references: missingRefs,
      raw_ids: [String(row['memory_id'])],
      files: (Array.isArray(row['file_references']) ? row['file_references'] as string[] : refs.filter(ref => ref.startsWith('file:')).map(ref => ref.slice('file:'.length))),
      created_at: String(row['created_at']),
      updated_at: row['updated_at'] ?? null,
    }, limit);
  }

  for (const row of developerReviewRows(db, withScope(params, { state: ['open', 'ongoing'], limit: 200 }))) {
    pushLimited(columns, counts, 'DeveloperReview', {
      item_type: String(row['source']) === 'refinement' ? 'refinement' : 'memory',
      id: String(row['id']),
      title: summarize(String(row['feedback']), 120),
      detail: summarize(String(row['context']), 180),
      agent_id: String(row['agent_id']),
      status: String(row['state']),
      raw_ids: [String(row['id'])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
      updated_at: row['updated_at'] ?? null,
    }, limit);
  }

  const pressure = inspectMaintenancePressure(db, {
    workspace: scopeFromParams(params).workspacePath,
    artifact: scopeFromParams(params).artifact,
    pressure_age_days: 1,
    workspace_normalized: true,
  });
  const pressureRows: AwarenessQueryRow[] = [];
  if (pressure.stale_pending_runs > 0) {
    const sample = pressure.samples.run_ids[0];
    pressureRows.push({
      item_type: 'pressure', id: 'stale-pending-runs', status: 'review',
      title: `${pressure.stale_pending_runs} pending run(s) older than ${pressure.pressure_age_days}d`,
      detail: 'Run the declared checks; pending age never implies success or deletion.',
      action: 'verify audit --workspace "$PWD" --compact',
      raw_ids: sample ? [sample] : [],
      files: [], created_at: utcNow(),
    });
  }
  if (pressure.stale_open_signals > 0) {
    pressureRows.push({
      item_type: 'pressure', id: 'stale-open-signals', status: 'review',
      title: `${pressure.stale_open_signals} open signal(s) older than ${pressure.pressure_age_days}d`,
      detail: 'Acknowledge or resolve after review; no signal is silently pruned.',
      action: 'signal list --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --all --limit 5 --compact',
      raw_ids: pressure.samples.signal_ids,
      files: [], created_at: utcNow(),
    });
  }
  if (pressure.stale_missing_refs > 0) {
    const memoryId = pressure.samples.memory_ids[0];
    pressureRows.push({
      item_type: 'pressure', id: 'stale-missing-memory-refs', status: 'review',
      title: `${pressure.stale_missing_refs} old memory reference(s) point to missing files`,
      detail: 'Revalidate, supersede, or preview deletion by exact memory id.',
      action: memoryId
        ? `memory forget --memory-id ${memoryId} --dry-run --compact`
        : 'query files --workspace "$PWD" --format table --limit 20',
      raw_ids: pressure.samples.memory_ids,
      files: [], created_at: utcNow(),
    });
  }
  for (const row of pressureRows) pushLimited(columns, counts, 'Maintenance', row, limit);

  const profile = Object.fromEntries(repoProfileRows(db, params).map(row => [String(row['metric']), Number(row['count'] ?? 0)])) as Record<string, number>;
  const activeMemories = Number(profile['active_memories'] ?? 0);
  const taskCount = Number(profile['tasks'] ?? 0);
  const allOpenRefinements = Number(profile['all_open_refinements'] ?? 0);
  const actionableRefinements = Number(profile['actionable_refinements'] ?? 0);
  const openSignalCount = Number(profile['open_signals'] ?? 0);
  const missingFileRefs = Number(profile['missing_file_refs'] ?? 0);
  const projectionWarnings = [
    missingFileRefs > 0 ? 'missing_file_refs' : null,
    activeMemories > 200 ? 'active_memories_over_200' : null,
    taskCount > 500 ? 'task_rows_over_500' : null,
    allOpenRefinements > 40 ? 'all_open_refinements_over_40' : null,
  ].filter((warning): warning is string => Boolean(warning));
  pushLimited(columns, counts, 'ProjectionHealth', {
    item_type: 'projection',
    id: 'projection-health',
    title: projectionWarnings.length > 0 ? 'Projection/bloat review suggested' : 'Projection health nominal',
    detail: projectionWarnings.join(', ') || 'No profile threshold warnings.',
    status: projectionWarnings.length > 0 ? 'review' : 'ok',
    count: projectionWarnings.length,
    raw_ids: [],
    files: [],
    active_memories: activeMemories,
    missing_file_refs: missingFileRefs,
    tasks: taskCount,
    actionable_refinements: actionableRefinements,
    all_open_refinements: allOpenRefinements,
    open_signals: openSignalCount,
    created_at: utcNow(),
  }, limit);

  // Detail reads are intentionally capped; totals are separate exact queries so
  // compact agents can trust the number and see how much detail was omitted.
  counts.Verify = countTaskRows(db, withScope(params, { state: ['VERIFY'] }))
    + countPendingStandaloneRuns(db, params);
  counts.Ready = countTaskRows(db, withScope(params, { state: ['OPEN'] }), true);
  counts.Claimed = countTaskRows(db, withScope(params, { state: ['IN_PROGRESS'] }));

  return Object.entries(columns).flatMap(([column, rows]) => {
    const total = counts[column] ?? rows.length;
    return rows.map(row => ({
      ...row,
      column_total: total,
      omitted_count: Math.max(0, total - rows.length),
    }));
  });
}
