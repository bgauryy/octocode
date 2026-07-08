/**
 * Shared agent-tool operation runner for awareness surfaces.
 *
 * Pi, future harnesses, and tests can call this directly instead of copying
 * memory/lock/signal/reflection orchestration into each host adapter.
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { fileLock } from './intents.js';
import { attendAwareness } from './attend.js';
import { digest, exportHarness, exportMemoryDoc, getWorkspaceStatus } from './maintenance.js';
import { findSimilarMemories, forgetMemory, getMemory, insertMemory, mineWeakness } from './memory.js';
import { agentSignal } from './notifications.js';
import { reflect as reflectMemory } from './reflect.js';
import { getRefinements } from './refinements.js';
import { injectRepoContext, queryAwareness, writeAwarenessView } from './repo-context.js';
import type {
  AgentSignalResult,
  FileLockResult,
  InsertMemoryResult,
  LockType,
  NotificationKind,
  TaskStatus,
} from './types.js';
import { auditUnverified, markVerified, type MarkVerifiedResult } from './verify.js';

export type AwarenessToolOperation =
  | 'recall'
  | 'record'
  | 'reflect'
  | 'workspace_status'
  | 'refine_get'
  | 'audit_unverified'
  | 'verify'
  | 'digest'
  | 'forget'
  | 'notify'
  | 'agent_signal'
  | 'file_lock'
  | 'mine_weakness'
  | 'export_harness'
  | 'attend'
  | 'query'
  | 'view'
  | 'repo_inject';

export interface AwarenessToolOperationContext {
  agentId?: string | null;
  cwd?: string | null;
  sessionId?: string | null;
}

export interface AwarenessToolOperationResult {
  payload: unknown;
  exitCode: number;
}

const DEFAULT_IMPORTANCE: Record<string, number> = {
  BUG: 8,
  GOTCHA: 7,
  IMPROVEMENT: 7,
  SECURITY: 9,
  INCIDENT: 9,
  RELEASE: 8,
  DECISION: 6,
  ARCHITECTURE: 6,
};

function defaultImportance(label: string | undefined): number {
  return DEFAULT_IMPORTANCE[label?.toUpperCase() ?? ''] ?? 5;
}

function normalizeSupersedes(value: unknown): string[] {
  if (Array.isArray(value)) return value as string[];
  return value ? [value as string] : [];
}

function requireText(
  params: Record<string, unknown>,
  key: string,
  type: string,
): string {
  const value = params[key];
  if (typeof value !== 'string' || value.trim().length === 0) {
    throw new Error(`memory ${type} requires ${key}`);
  }
  return value;
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string' && v.trim().length > 0) : [];
}

function scopeReferences(request: Record<string, unknown>): string[] {
  const references = stringArray(request['references']);
  const file = typeof request['file'] === 'string' ? [`file:${request['file']}`] : [];
  const files = stringArray(request['files']).map((p) => `file:${p}`);
  const folders = stringArray(request['folders']).map((p) => `dir:${p}`);
  return [...references, ...file, ...files, ...folders];
}

function recallQuery(request: Record<string, unknown>, type: string): string {
  // Only use semantic text for FTS search. File/folder/repo names are structural
  // scope filters and must not be appended to the search query.
  return requireText(request, 'query', type);
}

export function runAwarenessToolOperation(
  db: DatabaseSync,
  operation: AwarenessToolOperation,
  request: Record<string, unknown>,
  context: AwarenessToolOperationContext = {},
): AwarenessToolOperationResult {
  const cwd = context.cwd ?? process.cwd();
  const agentId = context.agentId ?? 'agent';

  switch (operation) {
    case 'recall': {
      const rawRefs = request['references'];
      const recallRefs = Array.isArray(rawRefs) ? rawRefs as string[] : rawRefs ? [String(rawRefs)] : [];
      const rawRegex = request['regex'];
      const recallRegex = Array.isArray(rawRegex) ? rawRegex as string[] : rawRegex ? [String(rawRegex)] : [];
      const result = getMemory(db, {
        query: recallQuery(request, operation),
        limit: (request['limit'] as number | undefined) ?? 3,
        minImportance: request['min_importance'] as number | undefined,
        label: request['label'] ? [(request['label'] as string)] : undefined,
        smart: request['smart'] as boolean | undefined,
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        globalOnly: request['global_only'] as boolean | undefined,
        strictScope: request['strict_scope'] as boolean | undefined,
        sort: request['sort'] as string | undefined,
        states: request['state'] ? [String(request['state'])] : undefined,
        references: recallRefs.length > 0 ? recallRefs : undefined,
        regex: recallRegex.length > 0 ? recallRegex : undefined,
        files: (() => {
          const merged = [...stringArray(request['files'])];
          if (typeof request['file'] === 'string' && request['file']) merged.push(request['file']);
          return merged.length > 0 ? merged : undefined;
        })(),
        asOf: request['as_of'] as string | undefined ?? null,
      });
      type MemRecord = {
        memory_id: string;
        observation?: string;
        task_context?: string;
        label?: string;
        importance?: number;
        score?: number;
        tags?: string[];
        references?: string[];
        failure_signature?: string;
        repo?: string;
        ref?: string;
      };
      const memories = (result.memories as MemRecord[]).map((m) => {
        const lean: Record<string, unknown> = {
          memory_id: m.memory_id,
          observation: m.observation,
          task_context: m.task_context,
          label: m.label,
          importance: m.importance,
          score: Math.round((m.score ?? 0) * 100) / 100,
        };
        if (m.tags?.length) lean['tags'] = m.tags;
        if (m.references?.length) lean['references'] = m.references;
        if (m.failure_signature) lean['failure_signature'] = m.failure_signature;
        if (m.repo) lean['repo'] = m.repo;
        if (m.ref) lean['ref'] = m.ref;
        const requestedFile = typeof request['file'] === 'string' ? resolve(cwd, request['file']) : null;
        const fileRefs = (m.references ?? [])
          .filter((ref) => ref.startsWith('file:') && !ref.startsWith('file://'))
          .map((ref) => ref.slice('file:'.length));
        const fileRef = requestedFile
          ? fileRefs.find((ref) => (isAbsolute(ref) ? resolve(ref) : resolve(cwd, ref)) === requestedFile) ?? fileRefs[0]
          : fileRefs[0];
        if (fileRef) lean['file'] = isAbsolute(fileRef) ? fileRef : resolve(cwd, fileRef);
        return lean;
      });
      const payload: Record<string, unknown> = { count: result.count, memories };
      if (result.judgment_required) {
        payload['judgment_required'] = true;
        payload['judgment_reason'] = result.judgment_reason;
      }
      return { payload, exitCode: 0 };
    }

    case 'record': {
      const taskContext = requireText(request, 'task_context', operation);
      const observation = requireText(request, 'observation', operation);
      const label = ((request['label'] as string | undefined)?.toUpperCase()) ?? 'OTHER';
      const supersedes = normalizeSupersedes(request['supersedes']);
      const similar = findSimilarMemories(db, `${taskContext} ${observation}`, 5);
      const unsupersededSimilar = (similar as Array<{ memory_id: string; similarity: number }>)
        .filter((m) => !supersedes.includes(m.memory_id));
      if (unsupersededSimilar.length > 0 && request['allow_similar'] !== true) {
        return {
          payload: {
            skipped: true,
            reason: 'similar_memory_exists',
            similar: unsupersededSimilar.map((m) => ({
              memory_id: m.memory_id,
              similarity: Math.round(m.similarity * 100) / 100,
            })),
            next: 'Do not record a duplicate. Pass supersedes with stale id(s), or allow_similar:true only for distinct new evidence.',
          },
          exitCode: 0,
        };
      }
      const { memory, superseded } = insertMemory(db, {
        agentId,
        taskContext,
        observation,
        importance: (request['importance'] as number | undefined) ?? defaultImportance(label),
        label,
        tags: (request['tags'] as string[] | undefined) ?? [],
        references: scopeReferences(request),
        supersedes,
        failureSignature: (request['failure_signature'] as string | undefined) ?? null,
        validFrom: (request['valid_from'] as string | undefined) ?? null,
        validTo: (request['valid_to'] as string | undefined) ?? null,
        workspacePath: request['workspace_path'] as string | undefined,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        cwd,
        preComputedSimilar: similar,
      }) as InsertMemoryResult;
      const payload: Record<string, unknown> = {
        memory_id: memory.memory_id,
        importance: memory.importance,
        label: memory.label,
      };
      if (typeof memory.novelty_score === 'number') {
        payload['novelty'] = Math.round(memory.novelty_score * 100) / 100;
      }
      if (similar.length) payload['similar'] = similar.map((m) => m.memory_id);
      if (superseded.length) payload['superseded'] = superseded;
      return { payload, exitCode: 0 };
    }

    case 'reflect': {
      if (
        !request['lesson'] &&
        !request['didnt_work'] &&
        !request['fix_repo'] &&
        !request['fix_harness'] &&
        !request['failure_signature']
      ) {
        throw new Error(
          'memory reflect needs a reusable lesson, failure, fix_repo, fix_harness, or failure_signature; skip routine status',
        );
      }
      const rawOutcome = request['outcome'];
      const outcome =
        rawOutcome === 'worked' || rawOutcome === 'partial' || rawOutcome === 'failed'
          ? rawOutcome
          : 'partial';
      const result = reflectMemory(db, {
        agentId,
        task: requireText(request, 'task', operation),
        outcome,
        lesson: request['lesson'] as string | undefined,
        worked: request['worked'] as string | undefined,
        didntWork: request['didnt_work'] as string | undefined,
        fixRepo: request['fix_repo'] as string | undefined,
        fixHarness: request['fix_harness'] as string | undefined,
        failureSignature: request['failure_signature'] as string | undefined,
        importance: request['importance'] as number | undefined,
        judgmentNote: request['judgment_note'] as string | undefined,
        duo: Boolean(request['duo']),
        evalFailures: Array.isArray(request['eval_failures'])
          ? request['eval_failures'] as Array<{ id: string; dimension?: string; failure_signature?: string; suggested_lesson?: string }>
          : undefined,
        references: request['references'] as string[] | undefined,
        file: request['file'] as string | undefined,
        files: request['files'] as string[] | undefined,
        folders: request['folders'] as string[] | undefined,
        validFrom: (request['valid_from'] as string | undefined) ?? null,
        validTo: (request['valid_to'] as string | undefined) ?? null,
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        cwd,
      }) as unknown as {
        outcome: string;
        learning_memory_id: string;
        novelty_score?: number;
        similar_memory_ids?: string[];
        repo_fix_refinement_id?: string;
        harness_fix?: boolean;
        eval_failure_count?: number;
        eval_failure_ids?: string[];
        reflection_duo?: unknown;
      };
      const payload: Record<string, unknown> = {
        outcome: result.outcome,
        memory_id: result.learning_memory_id,
      };
      if (typeof result.novelty_score === 'number' && result.novelty_score < 0.75) {
        payload['novelty'] = Math.round(result.novelty_score * 100) / 100;
      }
      if (result.similar_memory_ids?.length) payload['similar'] = result.similar_memory_ids;
      if (result.eval_failure_count) {
        payload['eval_failure_count'] = result.eval_failure_count;
        payload['eval_failure_ids'] = result.eval_failure_ids;
      }
      if (result.reflection_duo) payload['reflection_duo'] = result.reflection_duo;
      const actions: string[] = [];
      if (result.repo_fix_refinement_id) {
        payload['refinement_id'] = result.repo_fix_refinement_id;
        actions.push('memory_refine_get -> repo fixes for the next agent');
      }
      if (result.harness_fix) {
        payload['harness_fix'] = true;
        actions.push('harness improvement (a human merges)');
      }
      if (actions.length) payload['next'] = actions.join(' | ');
      return { payload, exitCode: 0 };
    }

    case 'workspace_status': {
      const result = getWorkspaceStatus(db, {
        workspace_path: (request['workspace_path'] as string | undefined) ?? cwd,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        cwd,
      });
      const payload: Record<string, unknown> = {
        active_memories: result.active_memories,
        pending_tasks: result.pending_tasks,
        active_tasks: result.active_tasks,
        open_refinements: result.open_refinements,
      };
      if (result.locks.length > 0) {
        payload['locks'] = result.locks.map((l) => ({
          file: l.file_path,
          task_id: l.task_id,
          agent: l.agent_id,
          type: l.lock_type,
          since: l.acquired_at,
          ...(l.expires_at ? { expires: l.expires_at } : {}),
        }));
      }
      return { payload, exitCode: 0 };
    }

    case 'refine_get': {
      const result = getRefinements(db, {
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        repo: request['repo'] as string | undefined,
        states: request['state'] ? [(request['state'] as string)] : undefined,
        includeHandoffs: Boolean(request['include_handoffs']),
        limit: (request['limit'] as number | undefined) ?? 5,
        cwd,
      }) as unknown as {
        refinements: Array<{
          refinement_id: string;
          state: string;
          remember: string;
          files?: string[];
          repo?: string;
        }>;
      };
      const refinements = result.refinements.map((r) => {
        const lean: Record<string, unknown> = {
          refinement_id: r.refinement_id,
          state: r.state,
          fix: r.remember,
        };
        if (r.files?.length) lean['files'] = r.files;
        if (r.repo) lean['repo'] = r.repo;
        return lean;
      });
      return { payload: { count: refinements.length, refinements }, exitCode: 0 };
    }

    case 'audit_unverified': {
      const result = auditUnverified(db, {
        agentId,
        workspacePath: cwd,
      }) as unknown as {
        unverified: Array<{ task_id: string; test_plan: string; target_files?: string[] }>;
        stale_active: Array<{ task_id: string; agent_id: string; age_hours: number; rationale: string; target_files?: string[] }>;
        count: number;
      };
      const pending = result.unverified.map((i) => {
        const lean: Record<string, unknown> = { task_id: i.task_id, test_plan: i.test_plan };
        if (i.target_files?.length) lean['files'] = i.target_files;
        return lean;
      });
      const stale = (result.stale_active ?? []).map((i) => {
        const lean: Record<string, unknown> = {
          task_id: i.task_id,
          agent_id: i.agent_id,
          age_hours: i.age_hours,
          reason: `ACTIVE task with no live locks (orphaned session) - ${i.rationale}`,
        };
        if (i.target_files?.length) lean['files'] = i.target_files;
        return lean;
      });
      const payload: Record<string, unknown> = { count: result.count, pending };
      if (stale.length > 0) payload['stale_active'] = stale;
      return { payload, exitCode: result.count ? 1 : 0 };
    }

    case 'verify': {
      const singleId = request['task_id'] as string | undefined;
      const batchIds = Array.isArray(request['task_ids']) ? (request['task_ids'] as unknown[]).map(String) : [];
      const allPending = Boolean(request['allPending']);
      const verifyStatus = ((request['status'] as string | undefined) ?? 'SUCCESS') as 'SUCCESS' | 'FAILED';

      if (allPending && !singleId && batchIds.length === 0) {
        const r = markVerified(db, { allPending: true, agentId, workspacePath: cwd, status: verifyStatus }) as MarkVerifiedResult;
        if (!r.ok) {
          return { payload: { task_id: r.task_id, error: r.error }, exitCode: 1 };
        }
        return { payload: { count: r.count, task_ids: r.task_ids ?? [], status: r.status }, exitCode: 0 };
      }

      const ids: string[] = [];
      if (singleId) ids.push(singleId);
      for (const id of batchIds) if (id && !ids.includes(id)) ids.push(id);
      if (allPending) {
        const pending = auditUnverified(db, { agentId, workspacePath: cwd }) as unknown as {
          unverified: Array<{ task_id: string }>;
        };
        for (const i of pending.unverified) if (!ids.includes(i.task_id)) ids.push(i.task_id);
      }

      if (ids.length === 0) {
        throw new Error('memory_verify requires task_id, task_ids[], or allPending:true');
      }

      const verifyResults = ids.map((taskId) => {
        const r = markVerified(db, { taskId, agentId, status: verifyStatus }) as MarkVerifiedResult;
        return r.ok
          ? { task_id: r.task_id, status: r.status }
          : { task_id: r.task_id, error: r.error };
      });

      const allOk = verifyResults.every((r) => !('error' in r));
      const payload = verifyResults.length === 1 ? verifyResults[0] : { count: verifyResults.length, results: verifyResults };
      return { payload, exitCode: allOk ? 0 : 1 };
    }

    case 'digest': {
      const digestParams: Record<string, unknown> = {
        retention_days: (request['retention_days'] as number | undefined) ?? 90,
      };
      if (request['workspace'] || request['workspace_path']) {
        digestParams['workspace'] = request['workspace'] ?? request['workspace_path'];
      }
      if (request['artifact']) digestParams['artifact'] = request['artifact'];
      if (request['dry_run']) digestParams['dry_run'] = true;
      const result = digest(db, digestParams);

      const payload: Record<string, unknown> = result.dry_run
        ? {
            dry_run: true,
            would_archive: result.would_archive,
            would_prune_old: result.would_prune_old,
            would_prune_locks: result.would_prune_locks,
            would_prune_refinements: result.would_prune_refinements,
          }
        : {
            archived_memories: result.archived_memories,
            pruned_old: result.pruned_old,
            pruned_locks: result.pruned_locks,
            pruned_refinements: result.pruned_refinements,
            fts_rebuilt: result.fts_rebuilt,
          };

      if (request['export_doc']) {
        try {
          const wsPath = (request['workspace_path'] as string | undefined) ?? cwd;
          const docDir = join(wsPath, '.octocode', 'memory-reports');
          mkdirSync(docDir, { recursive: true });
          const dateStr = new Date().toISOString().slice(0, 16).replace('T', '-').replace(':', '');
          const docPath = join(docDir, `memory-report-${dateStr}.md`);
          const mdContent = exportMemoryDoc(db, { workspace_path: wsPath });
          writeFileSync(resolve(docPath), mdContent, 'utf8');
          payload['doc_path'] = docPath;
        } catch (err) {
          payload['doc_warning'] = `Could not write doc: ${(err as Error).message}`;
        }
      }

      return { payload, exitCode: 0 };
    }

    case 'forget': {
      const rawIds = request['memory_id'] ?? request['memory_ids'];
      const memIds = Array.isArray(rawIds) ? rawIds as string[] : rawIds ? [String(rawIds)] : [];
      const rawTags = request['tags'];
      const forgTags = Array.isArray(rawTags) ? rawTags as string[] : rawTags ? [String(rawTags)] : [];
      const result = forgetMemory(db, {
        memoryIds: memIds,
        tags: forgTags,
        before: request['before'] as string | undefined,
        maxImportance: request['max_importance'] as number | undefined,
        workspacePath: request['workspace_path'] as string | undefined,
        artifact: request['artifact'] as string | undefined,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        dryRun: Boolean(request['dry_run']),
      });
      return { payload: result, exitCode: 0 };
    }

    case 'mine_weakness': {
      const result = mineWeakness(db, {
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        agentId: request['agent_id'] ? String(request['agent_id']) : undefined,
        minCount: (request['min_count'] as number | undefined) ?? 2,
        limit: (request['limit'] as number | undefined) ?? 10,
        cwd,
      });
      const clusters = result.clusters.map(c => ({
        signature: c.failure_signature,
        count: c.count,
        avg_importance: c.avg_importance,
        score: c.score,
        memory_ids: c.memory_ids,
        representative: c.representative,
        labels: c.labels,
      }));
      return {
        payload: {
          total_signatures: result.total_signatures,
          total_memories: result.total_memories,
          count: clusters.length,
          clusters,
          next: clusters.length > 0
            ? 'Use failure_signature values with memory_reflect to route lessons into fix_repo or fix_harness.'
            : 'No recurring failure patterns found. Record failures with failure_signature to build the cluster.',
        },
        exitCode: 0,
      };
    }

    case 'export_harness': {
      const result = exportHarness(db, {
        limit: (request['limit'] as number | undefined) ?? 10,
        min_importance: (request['min_importance'] as number | undefined) ?? 7,
        workspace_path: (request['workspace_path'] as string | undefined) ?? cwd,
        harness_only: Boolean(request['harness_only']),
      }) as unknown as { count: number; harness_count?: number; markdown: string; memories: Array<{ memory_id: string; label: string; importance: number; tier?: number; observation: string }> };
      const payload: Record<string, unknown> = {
        count: result.count,
        harness_count: result.harness_count,
        memories: result.memories.map(m => ({
          memory_id: m.memory_id,
          label: m.label,
          importance: m.importance,
          tier: m.tier,
          observation: m.observation.slice(0, 200),
        })),
        markdown: result.markdown,
      };
      if (result.count === 0) {
        payload['next'] = 'No harness proposals yet. Use memory_reflect with fix_harness: to propose skill improvements.';
      } else {
        payload['next'] = 'Review the markdown block, then paste harness-tier entries into AGENTS.md or CLAUDE.md after human approval.';
      }
      return { payload, exitCode: 0 };
    }

    case 'query': {
      const result = queryAwareness(db, {
        view: request['view'] as string | undefined,
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        artifact: request['artifact'] as string | undefined,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        query: request['query'] as string | undefined,
        limit: request['limit'] as number | undefined,
        agentId: request['agent_id'] as string | undefined,
        state: request['state'] as string | string[] | undefined,
        label: request['label'] as string | string[] | undefined,
        file: request['file'] as string | undefined,
        since: request['since'] as string | undefined,
        includeBodies: request['include_bodies'] as boolean | undefined,
        cwd,
      });
      return { payload: result, exitCode: 0 };
    }

    case 'attend': {
      const result = attendAwareness(db, {
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        artifact: request['artifact'] as string | undefined,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        query: request['query'] as string | undefined,
        limit: request['limit'] as number | undefined,
        file: request['file'] as string[] | string | undefined,
        includeBodies: request['include_bodies'] as boolean | undefined,
        explainOrgan: request['explain_organ'] as boolean | undefined,
        compact: request['compact'] as boolean | undefined,
        cwd,
      });
      return { payload: result, exitCode: 0 };
    }

    case 'view': {
      const result = writeAwarenessView(db, {
        view: request['view'] as string | undefined,
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        artifact: request['artifact'] as string | undefined,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        query: request['query'] as string | undefined,
        limit: request['limit'] as number | undefined,
        out: request['out'] as string | undefined,
        cwd,
      });
      return { payload: result, exitCode: 0 };
    }

    case 'repo_inject': {
      const result = injectRepoContext(db, {
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        artifact: request['artifact'] as string | undefined,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        query: request['query'] as string | undefined,
        limit: request['limit'] as number | undefined,
        outDir: request['out_dir'] as string | undefined,
        mode: request['mode'] as string | undefined,
        includeView: request['include_view'] as boolean | undefined,
        check: request['check'] as boolean | undefined,
        cwd,
      });
      return { payload: result, exitCode: 0 };
    }

    case 'notify': {
      const notifyKind = request['kind'] as string;
      const notifySubject = request['subject'] as string;
      if (!notifyKind || !notifySubject) throw new Error('memory notify requires kind and subject');
      const rawNFiles = request['files'];
      const notifyFiles = Array.isArray(rawNFiles) ? rawNFiles as string[] : [];
      const result = agentSignal(db, {
        action: 'publish',
        agentId,
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        toAgents: request['to_agent'] ? [String(request['to_agent'])] : [],
        kind: notifyKind as NotificationKind,
        subject: notifySubject,
        body: request['body'] as string | undefined ?? null,
        files: notifyFiles,
        importance: request['importance'] as number | undefined ?? 5,
        cwd,
      });
      return { payload: { ...result, alias: 'memory_notify', prefer: 'agent_signal' }, exitCode: 0 };
    }

    case 'agent_signal': {
      const rawAction = request['action'];
      if (rawAction !== 'publish' && rawAction !== 'list' && rawAction !== 'reply' && rawAction !== 'resolve' && rawAction !== 'ack') {
        throw new Error('agent_signal requires action: publish | list | reply | resolve | ack');
      }
      const toAgents = Array.isArray(request['to_agents']) ? request['to_agents'] as string[] : request['to_agent'] ? [String(request['to_agent'])] : [];
      const refs = Array.isArray(request['refs']) ? request['refs'] as string[] : [];
      const kinds = Array.isArray(request['kinds']) ? request['kinds'] as NotificationKind[] : [];
      const result = agentSignal(db, {
        action: rawAction,
        agentId: (request['agent_id'] as string | undefined) ?? agentId,
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        kind: request['kind'] as NotificationKind | undefined,
        subject: request['subject'] as string | undefined,
        body: request['body'] as string | undefined ?? null,
        toAgents,
        files: stringArray(request['files']),
        refs,
        importance: request['importance'] as number | undefined,
        inReplyTo: (request['in_reply_to'] as string | undefined) ?? null,
        threadId: (request['thread_id'] as string | undefined) ?? null,
        signalIds: stringArray(request['signal_ids']),
        unreadOnly: request['unread_only'] as boolean | undefined,
        markRead: request['mark_read'] as boolean | undefined,
        kinds,
        limit: request['limit'] as number | undefined,
        cwd,
      }) as AgentSignalResult;
      return { payload: result, exitCode: 0 };
    }

    case 'file_lock': {
      const rawType = request['type'];
      if (rawType !== 'lock' && rawType !== 'release' && rawType !== 'status' && rawType !== 'renew') {
        throw new Error('memory_file_lock requires type: lock | release | status | renew');
      }
      const lockAgentId = (request['agentId'] as string | undefined) ?? (request['agent_id'] as string | undefined) ?? agentId;
      const workspacePath = (request['workspace_path'] as string | undefined) ?? cwd;
      const targetFiles = (request['targetFiles'] as string[] | undefined) ?? (request['target_files'] as string[] | undefined) ?? [];
      const result = fileLock(db, {
        type: rawType,
        agentId: lockAgentId,
        sessionId: (request['sessionId'] as string | undefined) ?? (request['session_id'] as string | undefined) ?? context.sessionId ?? null,
        workspacePath,
        taskId: (request['taskId'] as string | undefined) ?? (request['task_id'] as string | undefined) ?? null,
        targetFiles,
        lockType: (request['lockType'] as LockType | undefined) ?? (request['lock_type'] as LockType | undefined),
        ttlMs: (request['ttlMs'] as number | undefined) ?? (request['ttl_ms'] as number | undefined) ?? null,
        status: request['status'] as TaskStatus | undefined,
        verified: request['verified'] as boolean | undefined,
        verifiedNote: (request['verifiedNote'] as string | undefined) ?? (request['verified_note'] as string | undefined),
        reasoning: request['reasoning'] as string | undefined,
      }) as FileLockResult;
      if (result.type === 'lock' && result.ok === false && request['signal_on_conflict'] !== false) {
        const conflictAgents = [...new Set(result.conflicts.map((conflict) => conflict.agent_id))];
        agentSignal(db, {
          action: 'publish',
          agentId: lockAgentId,
          workspacePath,
          toAgents: conflictAgents,
          kind: 'blocker',
          subject: `File lock conflict: ${targetFiles.slice(0, 3).join(', ') || 'target file'}`,
          body: JSON.stringify({ conflicts: result.conflicts }),
          files: targetFiles,
          importance: 7,
          cwd,
        });
      }
      return { payload: result, exitCode: result.ok === false ? 2 : 0 };
    }
  }
}
