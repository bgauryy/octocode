import type { DatabaseSync } from '@octocodeai/agent-contracts/sqlite';
import type { MigrationEvent } from './db-consolidation-handoffs.js';
import { signalExpiresAt } from './message-lifecycle.js';

type RefinementDestination = 'message' | 'work' | 'memory';
type RefinementBlocker =
  | 'INSTRUCTIONS_REQUIRE_AUTHORITY_DESTINATION'
  | 'MIXED_ACTION_AND_MEMORY_PAYLOAD'
  | 'INSUFFICIENT_DESTINATION_EVIDENCE'
  | 'INVALID_FILES_JSON';

interface LegacyRefinement {
  refinementId: string;
  agentId: string;
  workspacePath: string;
  artifact: string | null;
  repo: string | null;
  ref: string | null;
  files: string[];
  reasoning: string;
  remember: string;
  quality: 'good' | 'bad' | 'handoff' | 'instructions';
  state: 'open' | 'ongoing' | 'done';
  createdAt: string;
  updatedAt: string;
}

interface ClassifiedRefinement {
  row: LegacyRefinement;
  destination: RefinementDestination;
}

function hasRefinements(db: DatabaseSync): boolean {
  return Boolean(db.prepare("SELECT 1 FROM sqlite_schema WHERE type='table' AND name='refinements'").get());
}

function required(row: Record<string, unknown>, field: string): string {
  const value = row[field];
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`AMBIGUOUS_REFINEMENT ${String(row.refinement_id ?? '<unknown>')} INVALID_${field.toUpperCase()}`);
  }
  return value;
}

function optionalText(value: unknown, id: string, field: string): string | null {
  if (value === null) return null;
  if (typeof value !== 'string') throw new Error(`AMBIGUOUS_REFINEMENT ${id} INVALID_${field.toUpperCase()}`);
  return value;
}

function parseFiles(value: unknown, _id: string): string[] | 'INVALID_FILES_JSON' {
  if (typeof value !== 'string') return 'INVALID_FILES_JSON';
  try {
    const parsed: unknown = JSON.parse(value);
    if (!Array.isArray(parsed) || parsed.some((file) => typeof file !== 'string' || file.length === 0)) {
      return 'INVALID_FILES_JSON';
    }
    return [...new Set(parsed)];
  } catch {
    return 'INVALID_FILES_JSON';
  }
}

function classify(row: LegacyRefinement): RefinementDestination | RefinementBlocker {
  const hasFiles = row.files.length > 0;
  const hasMemory = row.remember.trim().length > 0;
  if (row.quality === 'handoff') return 'message';
  if (row.quality === 'instructions') return 'INSTRUCTIONS_REQUIRE_AUTHORITY_DESTINATION';
  if (hasFiles && hasMemory) return 'MIXED_ACTION_AND_MEMORY_PAYLOAD';
  if (row.state === 'done' && hasMemory) return 'memory';
  if (row.state !== 'done' && hasFiles) return 'work';
  return 'INSUFFICIENT_DESTINATION_EVIDENCE';
}

function readClassifiedRefinements(source: DatabaseSync): ClassifiedRefinement[] {
  if (!hasRefinements(source)) return [];
  const sourceRows = source.prepare('SELECT * FROM refinements ORDER BY created_at, refinement_id').all() as Array<Record<string, unknown>>;
  const classified: ClassifiedRefinement[] = [];
  const blockers: string[] = [];
  for (const sourceRow of sourceRows) {
    const refinementId = required(sourceRow, 'refinement_id');
    const files = parseFiles(sourceRow.files_json, refinementId);
    if (files === 'INVALID_FILES_JSON') {
      blockers.push(`${refinementId} ${files}`);
      continue;
    }
    const quality = required(sourceRow, 'quality');
    const state = required(sourceRow, 'state');
    if (!['good', 'bad', 'handoff', 'instructions'].includes(quality)
      || !['open', 'ongoing', 'done'].includes(state)) {
      blockers.push(`${refinementId} INVALID_CLASSIFICATION_FIELDS`);
      continue;
    }
    const row: LegacyRefinement = {
      refinementId,
      agentId: required(sourceRow, 'agent_id'),
      workspacePath: required(sourceRow, 'workspace_path'),
      artifact: optionalText(sourceRow.artifact, refinementId, 'artifact'),
      repo: optionalText(sourceRow.repo, refinementId, 'repo'),
      ref: optionalText(sourceRow.ref, refinementId, 'ref'),
      files,
      reasoning: required(sourceRow, 'reasoning'),
      remember: typeof sourceRow.remember === 'string' ? sourceRow.remember : '',
      quality: quality as LegacyRefinement['quality'],
      state: state as LegacyRefinement['state'],
      createdAt: required(sourceRow, 'created_at'),
      updatedAt: required(sourceRow, 'updated_at'),
    };
    const destination = classify(row);
    if (destination === 'message' || destination === 'work' || destination === 'memory') {
      classified.push({ row, destination });
    } else blockers.push(`${refinementId} ${destination}`);
  }
  if (blockers.length > 0) {
    throw new Error(`AMBIGUOUS_REFINEMENT: ${blockers.join('; ')}; source has not been changed and no destination was created`);
  }
  return classified;
}

export function assertClassifiableRefinements(source: DatabaseSync): void {
  readClassifiedRefinements(source);
}

export function refinementMigrationEvents(source: DatabaseSync): MigrationEvent[] {
  return readClassifiedRefinements(source).map(({ row, destination }) => ({
    event_id: `legacy.refinement:${row.refinementId}`,
    workspace_path: row.workspacePath,
    event_type: destination === 'message' ? 'peer.message' : destination === 'work' ? 'work.migrated' : 'memory.recorded',
    aggregate_kind: destination,
    aggregate_id: `migrated.refinement:${row.refinementId}`,
    aggregate_revision: null,
    actor_json: JSON.stringify({ kind: 'agent', id: row.agentId }),
    provenance_json: JSON.stringify({ source: 'migration', trust: 'attributed-data' }),
    payload_json: JSON.stringify({
      legacyRefinementId: row.refinementId,
      destination,
      quality: row.quality,
      state: row.state,
      reasoning: row.reasoning,
      remember: row.remember,
      files: row.files,
      artifact: row.artifact,
      repo: row.repo,
      ref: row.ref,
      updatedAt: row.updatedAt,
    }),
    session_id: null,
    correlation_id: null,
    created_at: row.createdAt,
    expires_at: null,
    schema_version: 1,
    retention_class: destination === 'message' ? 'delivery' : 'operational',
  }));
}

export function refinementDestinationCounts(source: DatabaseSync): Readonly<Record<string, number>> {
  const counts: Record<string, number> = {
    awareness_memories: 0,
    task_runs: 0,
    run_files: 0,
    signals: 0,
  };
  for (const { row, destination } of readClassifiedRefinements(source)) {
    if (destination === 'memory') counts.awareness_memories! += 1;
    else if (destination === 'message') counts.signals! += 1;
    else {
      counts.task_runs! += 1;
      counts.run_files! += row.files.length;
    }
  }
  return counts;
}

export function copyClassifiedRefinements(source: DatabaseSync, destination: DatabaseSync): number {
  const rows = readClassifiedRefinements(source);
  const insertSignal = destination.prepare(`INSERT INTO signals
    (signal_id,workspace_path,artifact,repo,ref,from_agent,to_agent,kind,subject,body,files_json,refs_json,
      thread_id,reply_to,importance,status,resolved_at,created_at,expires_at)
    VALUES (?,?,?,?,?,?,NULL,'handoff',?,?,?,?,?,NULL,5,?,?,?,?)`);
  const insertWork = destination.prepare(`INSERT INTO task_runs
    (run_id,task_id,origin,agent_id,session_id,rationale,test_plan,context_ref,status,workspace_path,artifact,created_at,updated_at)
    VALUES (?,NULL,'WORK',?,NULL,?,?,?,?,?,?,?,?)`);
  const insertFile = destination.prepare(`INSERT INTO run_files
    (run_id,file_path,reason_override,source,started_at,heartbeat_at,expires_at,ended_at)
    VALUES (?,?,NULL,'EXPLICIT',?,?,?,?)`);
  const insertMemory = destination.prepare(`INSERT INTO awareness_memories
    (memory_id,agent_id,task_context,observation,importance,state,label,tags_json,workspace_path,artifact,repo,ref,
      valid_from,scope_kind,source_digest,verified_at,created_at,updated_at)
    VALUES (?,?,?,?,5,'ACTIVE','OTHER','["migrated-refinement"]',?,?,?,?,?,?,?,?,?,?)`);

  for (const { row, destination: owner } of rows) {
    const id = `migrated.refinement:${row.refinementId}`;
    if (owner === 'message') {
      const resolvedAt = row.state === 'done' ? row.updatedAt : null;
      insertSignal.run(id, row.workspacePath, row.artifact, row.repo, row.ref, row.agentId,
        `Migrated refinement ${row.refinementId}`,
        row.remember.trim() ? `${row.reasoning}\n\n${row.remember}` : row.reasoning,
        JSON.stringify(row.files),
        JSON.stringify([`legacy-refinement:${row.refinementId}`]), id,
        resolvedAt ? 'resolved' : 'open', resolvedAt, row.createdAt,
        signalExpiresAt('handoff', row.createdAt));
      continue;
    }
    if (owner === 'work') {
      insertWork.run(id, row.agentId, row.reasoning,
        'Legacy refinement did not record verification criteria.', `legacy-refinement:${row.refinementId}`,
        row.state === 'ongoing' ? 'ACTIVE' : 'PENDING', row.workspacePath, row.artifact, row.createdAt, row.updatedAt);
      for (const file of row.files) {
        insertFile.run(id, file, row.createdAt, row.updatedAt, row.updatedAt, row.updatedAt);
      }
      continue;
    }
    insertMemory.run(id, row.agentId, row.reasoning, row.remember, row.workspacePath, row.artifact, row.repo, row.ref,
      row.createdAt, row.artifact ? 'artifact' : 'project',
      `legacy-refinement:${row.refinementId}`, row.updatedAt, row.createdAt, row.updatedAt);
  }
  return rows.length;
}
