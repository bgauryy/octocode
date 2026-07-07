/**
 * Memory tools — direct calls into @octocodeai/octocode-awareness (no subprocess).
 * Public Pi surface is split by operation; implementation stays shared.
 */
import {
  connectDb,
  resolveDbPath,
  getMemory,
  insertMemory,
  findSimilarMemories,
  reflect as reflectMemory,
  getRefinements,
  auditUnverified,
  markVerified,
  digest,
  getWorkspaceStatus,
  exportMemoryDoc,
  exportHarness,
  mineWeakness,
  forgetMemory,
  agentSignal,
  fileLock,
  getPiAwarenessSessionId,
} from '@octocodeai/octocode-awareness';
import type {
  AgentSignalResult,
  FileLockResult,
  InsertMemoryResult,
  LockType,
  MarkVerifiedResult,
  NotificationKind,
  TaskStatus,
} from '@octocodeai/octocode-awareness';
import { writeFileSync, mkdirSync } from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';
import type { PiContext, PiTheme, ToolDefinition, ToolCallResult } from '../types.js';
import { buildMemoryRenderCall, buildMemoryRenderResult } from './render-helpers.js';
import { stringEnumSchema } from './schema-helpers.js';
import type { registerUniqueTool } from './octocode-tools.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type Notifier = (ctx: PiContext | undefined, msg: string, level?: string) => void;
type AgentIdResolver = (ctx: PiContext | undefined) => string;
type RegisterFn = typeof registerUniqueTool;
type DbType = ReturnType<typeof connectDb>;

// TOOL-1: Module-level DB cache keyed by dbPath.
// withMemoryDb previously called connectDb (which runs initDb with all migration
// PRAGMAs) on every tool call. For a session with many memory_recall/record calls
// this is extremely expensive. A cached connection is safe: node:sqlite
// DatabaseSync is single-threaded and the module lives in one Node.js worker.
const _piToolDbCache = new Map<string, DbType>();

function cachedConnectDb(dbPath: string): DbType {
  const cached = _piToolDbCache.get(dbPath);
  if (cached) return cached;
  const db = connectDb(dbPath);
  _piToolDbCache.set(dbPath, db);
  return db;
}

const MEMORY_LABEL_VALUES = [
  'BUG',
  'FEATURE',
  'SUGGESTION',
  'GOTCHA',
  'IMPROVEMENT',
  'DECISION',
  'ARCHITECTURE',
  'SECURITY',
  'PERFORMANCE',
  'TEST',
  'BUILD',
  'DOCS',
  'CONFIG',
  'WORKFLOW',
  'REFACTOR',
  'API',
  'RELEASE',
  'INCIDENT',
  'EXPERIENCE',
  'OTHER',
] as const;
const MEMORY_LABELS = MEMORY_LABEL_VALUES.join('|');
const MEMORY_STATES = ['ACTIVE', 'SUPERSEDED'] as const;
const RECALL_SORTS = ['smart', 'importance', 'recent', 'accessed'] as const;
const REFINEMENT_STATES = ['open', 'ongoing', 'done'] as const;
const NOTIFICATION_KINDS = ['claim', 'handoff', 'question', 'reply', 'blocker', 'request', 'decision', 'fyi'] as const;
const FILE_LOCK_TYPES = ['lock', 'release', 'status', 'renew'] as const;
const FILE_LOCK_KINDS = ['EXCLUSIVE', 'SHARED'] as const;
const AGENT_SIGNAL_ACTIONS = ['publish', 'list', 'reply', 'resolve', 'ack'] as const;

export type MemoryType =
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
  | 'export_harness';

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
  // TOOL-4: Only use the semantic query text for FTS search. File paths, folder
  // paths, and repo names are STRUCTURAL scope filters — they must be passed as
  // workspacePath/files params to getMemory, not appended to the FTS query string.
  // Mixing filesystem paths into the FTS query corrupts BM25 ranking: a search
  // for 'react' would match any memory whose workspace_path contains 'react'.
  return requireText(request, 'query', type);
}

function runMemoryOperation(
  db: DbType,
  type: MemoryType,
  request: Record<string, unknown>,
  cwd: string,
  getAgentId: AgentIdResolver,
  ctx: PiContext | undefined,
): ToolCallResult {
  switch (type) {
    case 'recall': {
      const rawRefs = request['references'];
      const recallRefs = Array.isArray(rawRefs) ? rawRefs as string[] : rawRefs ? [String(rawRefs)] : [];
      const rawRegex = request['regex'];
      const recallRegex = Array.isArray(rawRegex) ? rawRegex as string[] : rawRegex ? [String(rawRegex)] : [];
      const result = getMemory(db, {
        query: recallQuery(request, type),
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
        // File-scope filter (getMemory supports `files`). The schema advertises
        // file/files; forward them (folder-scope isn't supported upstream).
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
      const recallPayload: Record<string, unknown> = { count: result.count, memories };
      // Low-confidence recall flag from getMemory — the agent should verify
      // weak matches against current files instead of trusting them.
      if (result.judgment_required) {
        recallPayload['judgment_required'] = true;
        recallPayload['judgment_reason'] = result.judgment_reason;
      }
      return {
        content: [{ type: 'text', text: JSON.stringify(recallPayload) }],
        details: { exit: 0 },
      };
    }

    case 'record': {
      const taskContext = requireText(request, 'task_context', type);
      const observation = requireText(request, 'observation', type);
      const label = ((request['label'] as string | undefined)?.toUpperCase()) ?? 'OTHER';
      const supersedes = normalizeSupersedes(request['supersedes']);
      // TOOL-2 note: findSimilarMemories here is a pre-flight dedup GATE.
      // insertMemory also calls findSimilarMemories internally for novelty metadata.
      // These serve different purposes (gate vs. metadata) so the double call is
      // accepted until insertMemory supports a preComputedSimilar param.
      const similar = findSimilarMemories(db, `${taskContext} ${observation}`, 5);
      const unsupersededSimilar = (similar as Array<{ memory_id: string; similarity: number }>)
        .filter((m) => !supersedes.includes(m.memory_id));
      if (unsupersededSimilar.length > 0 && request['allow_similar'] !== true) {
        return {
          content: [{
            type: 'text',
            text: JSON.stringify({
              skipped: true,
              reason: 'similar_memory_exists',
              similar: unsupersededSimilar.map((m) => ({
                memory_id: m.memory_id,
                similarity: Math.round(m.similarity * 100) / 100,
              })),
              next: 'Do not record a duplicate. Pass supersedes with stale id(s), or allow_similar:true only for distinct new evidence.',
            }),
          }],
          details: { exit: 0 },
        };
      }
      const { memory, superseded } = insertMemory(db, {
        agentId: getAgentId(ctx),
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
      return {
        content: [{ type: 'text', text: JSON.stringify(payload) }],
        details: { exit: 0 },
      };
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
        agentId: getAgentId(ctx),
        task: requireText(request, 'task', type),
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
        actions.push('memory_refine_get → repo fixes for the next agent');
      }
      if (result.harness_fix) {
        payload['harness_fix'] = true;
        actions.push('harness improvement (a human merges)');
      }
      if (actions.length) payload['next'] = actions.join(' · ');
      return {
        content: [{ type: 'text', text: JSON.stringify(payload) }],
        details: { exit: 0 },
      };
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
      return {
        content: [{ type: 'text', text: JSON.stringify(payload) }],
        details: { exit: 0 },
      };
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
      return {
        content: [{ type: 'text', text: JSON.stringify({ count: refinements.length, refinements }) }],
        details: { exit: 0 },
      };
    }

    case 'audit_unverified': {
      const result = auditUnverified(db, {
        agentId: getAgentId(ctx),
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
      // VER-2: Include stale ACTIVE tasks (orphaned sessions) in audit output
      const stale = (result.stale_active ?? []).map((i) => {
        const lean: Record<string, unknown> = {
          task_id: i.task_id,
          agent_id: i.agent_id,
          age_hours: i.age_hours,
          reason: `ACTIVE task with no live locks (orphaned session) — ${i.rationale}`,
        };
        if (i.target_files?.length) lean['files'] = i.target_files;
        return lean;
      });
      const payload: Record<string, unknown> = { count: result.count, pending };
      if (stale.length > 0) payload['stale_active'] = stale;
      return {
        content: [{ type: 'text', text: JSON.stringify(payload) }],
        details: { exit: result.count ? 1 : 0 },
      };
    }

    case 'verify': {
      const singleId = request['task_id'] as string | undefined;
      const batchIds = Array.isArray(request['task_ids']) ? (request['task_ids'] as unknown[]).map(String) : [];
      const allPending = Boolean(request['allPending']);
      const verifyStatus = ((request['status'] as string | undefined) ?? 'SUCCESS') as 'SUCCESS' | 'FAILED';
      const agentId = getAgentId(ctx);

      // TOOL-3: allPending=true delegates to markVerified(allPending:true) which runs
      // a single UPDATE batch query instead of auditUnverified + N individual markVerified
      // calls. Single + batch IDs are still handled individually so callers get
      // per-task results for those.
      if (allPending && !singleId && batchIds.length === 0) {
        const r = markVerified(db, { allPending: true, agentId, workspacePath: cwd, status: verifyStatus }) as MarkVerifiedResult;
        if (!r.ok) {
          return {
            content: [{ type: 'text', text: JSON.stringify({ task_id: r.task_id, error: r.error }) }],
            details: { exit: 1 },
          };
        }
        return {
          content: [{ type: 'text', text: JSON.stringify({ count: r.count, task_ids: r.task_ids ?? [], status: r.status }) }],
          details: { exit: 0 },
        };
      }

      // Collect IDs from explicit single + batch sources, deduplicating.
      const ids: string[] = [];
      if (singleId) ids.push(singleId);
      for (const id of batchIds) if (id && !ids.includes(id)) ids.push(id);
      if (allPending) {
        // Mixed mode: allPending + explicit IDs — gather pending and merge.
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
      // Backward compat: single-ID calls get the same flat payload shape
      const payload = verifyResults.length === 1 ? verifyResults[0] : { count: verifyResults.length, results: verifyResults };
      return {
        content: [{ type: 'text', text: JSON.stringify(payload) }],
        details: { exit: allOk ? 0 : 1 },
      };
    }
    case 'digest': {
      const digestParams: Record<string, unknown> = {
        retention_days: (request['retention_days'] as number | undefined) ?? 90,
      };
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

      return {
        content: [{ type: 'text', text: JSON.stringify(payload) }],
        details: { exit: 0 },
      };
    }

    case 'forget': {
      const rawIds = request['memory_ids'];
      const memIds = Array.isArray(rawIds) ? rawIds as string[] : rawIds ? [String(rawIds)] : [];
      const rawTags = request['tags'];
      const forgTags = Array.isArray(rawTags) ? rawTags as string[] : rawTags ? [String(rawTags)] : [];
      const result = forgetMemory(db, {
        memoryIds: memIds,
        tags: forgTags,
        before: request['before'] as string | undefined,
        maxImportance: request['max_importance'] as number | undefined,
        dryRun: Boolean(request['dry_run']),
      });
      return {
        content: [{ type: 'text', text: JSON.stringify(result) }],
        details: { exit: 0 },
      };
    }

    case 'mine_weakness': {
      // R-4: mineWeakness was CLI-only (mine-weakness command). Now a Pi tool.
      // Clusters memories by failure_signature and ranks by support × avg-importance.
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
        content: [{ type: 'text', text: JSON.stringify({
          total_signatures: result.total_signatures,
          total_memories: result.total_memories,
          count: clusters.length,
          clusters,
          next: clusters.length > 0
            ? 'Use failure_signature values with memory_reflect to route lessons into fix_repo or fix_harness.'
            : 'No recurring failure patterns found. Record failures with failure_signature to build the cluster.',
        }) }],
        details: { exit: 0 },
      };
    }

    case 'export_harness': {
      // Surfaces harness-tagged memories (tier 1) and high-importance lessons (tier 2).
      // Output is markdown ready to paste into AGENTS.md or CLAUDE.md.
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
      return {
        content: [{ type: 'text', text: JSON.stringify(payload) }],
        details: { exit: 0 },
      };
    }

    case 'notify': {
      const notifyKind = request['kind'] as string;
      const notifySubject = request['subject'] as string;
      if (!notifyKind || !notifySubject) throw new Error('memory notify requires kind and subject');
      const rawNFiles = request['files'];
      const notifyFiles = Array.isArray(rawNFiles) ? rawNFiles as string[] : [];
      const result = agentSignal(db, {
        action: 'publish',
        agentId: getAgentId(ctx),
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
      return {
        content: [{ type: 'text', text: JSON.stringify({ ...result, alias: 'memory_notify', prefer: 'agent_signal' }) }],
        details: { exit: 0 },
      };
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
        agentId: (request['agent_id'] as string | undefined) ?? getAgentId(ctx),
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
      return {
        content: [{ type: 'text', text: JSON.stringify(result) }],
        details: { exit: 0 },
      };
    }

    case 'file_lock': {
      const rawType = request['type'];
      if (rawType !== 'lock' && rawType !== 'release' && rawType !== 'status' && rawType !== 'renew') {
        throw new Error('memory_file_lock requires type: lock | release | status | renew');
      }
      const agentId = (request['agentId'] as string | undefined) ?? (request['agent_id'] as string | undefined) ?? getAgentId(ctx);
      const workspacePath = (request['workspace_path'] as string | undefined) ?? cwd;
      const targetFiles = (request['targetFiles'] as string[] | undefined) ?? (request['target_files'] as string[] | undefined) ?? [];
      const result = fileLock(db, {
        type: rawType,
        agentId,
        sessionId: (request['sessionId'] as string | undefined) ?? (request['session_id'] as string | undefined) ?? getPiAwarenessSessionId(ctx),
        workspacePath,
        taskId: (request['taskId'] as string | undefined) ?? (request['task_id'] as string | undefined) ?? null,
        targetFiles,
        lockType: request['lockType'] as LockType | undefined ?? request['lock_type'] as LockType | undefined,
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
          agentId,
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
      return {
        content: [{ type: 'text', text: JSON.stringify(result) }],
        details: { exit: result.ok === false ? 2 : 0 },
      };
    }
  }
}

function withMemoryDb(
  type: MemoryType,
  params: Record<string, unknown>,
  getAgentId: AgentIdResolver,
  ctx: PiContext | undefined,
): ToolCallResult {
  // TOOL-1: Use cached connection — avoids re-running initDb on every tool call.
  const db = cachedConnectDb(ctx?.dbPath ?? resolveDbPath(null));
  const cwd = ctx?.cwd ?? process.cwd();
  return runMemoryOperation(db, type, params, cwd, getAgentId, ctx);
}

export function executeMemoryOperation(
  type: MemoryType,
  params: Record<string, unknown>,
  getAgentId: AgentIdResolver,
  ctx?: PiContext,
): ToolCallResult {
  try {
    return withMemoryDb(type, params, getAgentId, ctx);
  } catch (err) {
    return {
      content: [{ type: 'text', text: `Failed: ${(err as Error).message}` }],
      details: { exit: 1 },
    };
  }
}

function optionalLimit(Type: TypeBoxBuilder, description: string): Record<string, unknown> {
  return Type.Optional(Type.Integer({ minimum: 1, maximum: 20, description }));
}

function nonEmptyString(Type: TypeBoxBuilder, description: string): Record<string, unknown> {
  return Type.String({ minLength: 1, description });
}

function optionalNonEmptyString(Type: TypeBoxBuilder, description: string): Record<string, unknown> {
  return Type.Optional(nonEmptyString(Type, description));
}

function optionalStringArray(Type: TypeBoxBuilder, description: string): Record<string, unknown> {
  return Type.Optional(Type.Array(nonEmptyString(Type, description), { description }));
}

function registerMemoryTool(
  getAgentId: AgentIdResolver,
  registerFn: RegisterFn,
  pi: { registerTool?(def: ToolDefinition): void },
  registeredToolNames: Set<string>,
  tool: {
    name: string;
    type: MemoryType;
    label: string;
    description: string;
    promptGuidelines: string[];
    parameters: ToolDefinition['parameters'];
  },
): void {
  registerFn(pi, registeredToolNames, {
    name: tool.name,
    label: tool.label,
    description: tool.description,
    promptSnippet: tool.description,
    promptGuidelines: tool.promptGuidelines,
    parameters: tool.parameters,
    async execute(
      _toolCallId: string,
      params: Record<string, unknown>,
      _signal?: AbortSignal,
      _onUpdate?: unknown,
      ctx?: PiContext,
    ): Promise<ToolCallResult> {
      return executeMemoryOperation(tool.type, params, getAgentId, ctx);
    },
    renderCall(args: unknown, theme?: PiTheme) {
      return buildMemoryRenderCall(tool.name, args, theme);
    },
    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      return buildMemoryRenderResult(tool.name, result, opts, theme);
    },
  });
}

export function buildMemoryToolDefinition(
  Type: TypeBoxBuilder,
  getAgentId: AgentIdResolver,
  registerFn: RegisterFn,
  pi: { registerTool?(def: ToolDefinition): void },
  registeredToolNames: Set<string>,
  _notify: Notifier,
): void {
  const labelSchema = Type.Optional(stringEnumSchema(
    Type,
    MEMORY_LABEL_VALUES,
    `Memory category. Allowed: ${MEMORY_LABELS}.`,
  ));
  const importanceSchema = Type.Optional(Type.Integer({
    minimum: 1,
    maximum: 10,
    description: '1–3 minor, 4–6 useful, 7–8 important, 9–10 critical.',
  }));
  const outcomeSchema = stringEnumSchema(Type, ['worked', 'partial', 'failed'], 'worked|partial|failed.');
  const verifyStatusSchema = stringEnumSchema(Type, ['SUCCESS', 'FAILED'], 'SUCCESS or FAILED; default SUCCESS.');
  const memoryStateSchema = stringEnumSchema(Type, MEMORY_STATES, 'ACTIVE (default) or SUPERSEDED.');
  const recallSortSchema = stringEnumSchema(Type, RECALL_SORTS, 'smart (default), importance, recent, or accessed.');
  const refinementStateSchema = stringEnumSchema(Type, REFINEMENT_STATES, 'open|ongoing|done. Default open/ongoing.');
  const notificationKindSchema = stringEnumSchema(Type, NOTIFICATION_KINDS, 'claim|handoff|question|reply|blocker|request|decision|fyi.');
  const fileLockTypeSchema = stringEnumSchema(Type, FILE_LOCK_TYPES, 'lock|release|status|renew.');
  const fileLockKindSchema = stringEnumSchema(Type, FILE_LOCK_KINDS, 'EXCLUSIVE or SHARED; default EXCLUSIVE.');
  const agentSignalActionSchema = stringEnumSchema(Type, AGENT_SIGNAL_ACTIONS, 'publish|list|reply|resolve|ack.');
  const fileScopeProps = {
    file: optionalNonEmptyString(Type, 'Primary related file path.'),
    files: optionalStringArray(Type, 'Related file paths.'),
    folders: optionalStringArray(Type, 'Related folder paths.'),
  };
  // Recall only supports file-scope on the read side (getMemory accepts `files`,
  // not folders) — advertise only what actually filters.
  const recallFileScopeProps = {
    file: optionalNonEmptyString(Type, 'Related file path to scope recall to.'),
    files: optionalStringArray(Type, 'Related file paths to scope recall to.'),
  };
  const workspaceScopeProp = {
    workspace_path: optionalNonEmptyString(Type, 'Workspace/repo root scope; defaults to cwd.'),
  };
  const repoRefProps = {
    repo: optionalNonEmptyString(Type, 'Repository scope, e.g. owner/repo.'),
    ref: optionalNonEmptyString(Type, 'Git ref/branch scope.'),
  };
  // Full write-side scope (workspace + repo + ref) for tools that persist scope.
  const repoScopeProps = { ...workspaceScopeProp, ...repoRefProps };
  const validityProps = {
    valid_from: optionalNonEmptyString(Type, 'Memory valid-from timestamp/ISO date.'),
    valid_to: optionalNonEmptyString(Type, 'Memory expiry timestamp/ISO date; digest marks expired memories stale.'),
  };

  // Shared schema objects — defined once, referenced by canonical + alias tools.
  // Previously file_lock and memory_file_lock each copy-pasted all 12 params.
  const fileLockParams = Type.Object({
    type: fileLockTypeSchema,
    target_files: optionalStringArray(Type, 'Files to lock or release. Relative paths resolve under workspace_path/cwd.'),
    task_id: optionalNonEmptyString(Type, 'Precise task id returned by type:lock. Required for safe release/renew.'),
    lock_type: Type.Optional(fileLockKindSchema),
    ttl_ms: Type.Optional(Type.Integer({ minimum: 1, description: 'Requested lock TTL in milliseconds; capped by awareness.' })),
    reasoning: optionalNonEmptyString(Type, 'Why this lock is needed; shown in lock/status output.'),
    agent_id: optionalNonEmptyString(Type, 'Agent id override; defaults to current Pi agent id.'),
    session_id: optionalNonEmptyString(Type, 'Session id override; defaults to current Pi session id.'),
    status: Type.Optional(Type.String({ description: 'Release status: PENDING, SUCCESS, or FAILED.' })),
    verified: Type.Optional(Type.Boolean({ description: 'For release: mark SUCCESS only if verification actually ran.' })),
    verified_note: optionalNonEmptyString(Type, 'Verification note stored with verified releases.'),
    signal_on_conflict: Type.Optional(Type.Boolean({ description: 'Publish a blocker signal on lock conflict; default true.' })),
    // fileLock scopes by workspace only — repo/ref are not honored upstream.
    ...workspaceScopeProp,
  });

  const workspaceStatusParams = Type.Object({ ...repoScopeProps });

  const tools = [
    {
      name: 'memory_recall',
      type: 'recall' as const,
      label: 'Memory: Recall',
      description: 'Recall durable lessons before risky, unfamiliar, or long-running work.',
      promptGuidelines: [
        'Use before the work only when prior lessons could change the plan.',
        'Skip for routine tasks, obvious one-step edits, or facts already in context.',
      ],
      parameters: Type.Object({
        query: nonEmptyString(Type, 'What you are about to work on, in natural language.'),
        limit: optionalLimit(Type, 'Max memories; default 3.'),
        min_importance: Type.Optional(Type.Integer({ minimum: 1, maximum: 10, description: 'Raise to filter low-signal noise.' })),
        smart: Type.Optional(Type.Boolean({ description: 'Broaden after zero results.' })),
        label: labelSchema,
        global_only: Type.Optional(Type.Boolean({ description: 'Search global memories only; skip workspace filtering.' })),
        strict_scope: Type.Optional(Type.Boolean({ description: 'Exact workspace match only; skip NULL-workspace global memories.' })),
        sort: Type.Optional(recallSortSchema),
        state: Type.Optional(memoryStateSchema),
        references: optionalStringArray(Type, 'Filter by exact provenance reference; e.g. npm:pkg, pr:owner/repo#N.'),
        regex: optionalStringArray(Type, 'Regex patterns matched against all text fields.'),
        as_of: optionalNonEmptyString(Type, 'ISO date for bi-temporal point-in-time recall.'),
        // Recall filters by file-scope + workspace only (getMemory ignores repo/ref/folders).
        ...recallFileScopeProps,
        ...workspaceScopeProp,
      }),
    },
    {
      name: 'memory_record',
      type: 'record' as const,
      label: 'Memory: Record',
      description: 'Store a durable root cause, decision, workaround, or verified gotcha.',
      promptGuidelines: [
        'Record only reusable findings that can change future work.',
        'Never store routine status, secrets, raw logs, test output, or facts already in git/docs.',
        'Use supersedes for stale duplicates; allow_similar only for genuinely distinct evidence.',
        'Prefer memory_reflect for post-task lessons — it also creates repo-fix refinements and clusters failure patterns automatically.',
      ],
      parameters: Type.Object({
        task_context: nonEmptyString(Type, 'Why a future agent needs this lesson.'),
        observation: nonEmptyString(Type, 'Durable lesson: X caused Y because Z — do A; verify with B.'),
        label: labelSchema,
        importance: importanceSchema,
        tags: optionalStringArray(Type, 'Recall keywords.'),
        references: optionalStringArray(Type, 'Provenance such as file:/abs/path:line, pr:owner/repo#N, URL, npm:pkg@v.'),
        ...fileScopeProps,
        ...repoScopeProps,
        ...validityProps,
        supersedes: Type.Optional(Type.Union([
          nonEmptyString(Type, 'Stale memory id this one replaces.'),
          Type.Array(nonEmptyString(Type, 'Stale memory id this one replaces.'), { description: 'Stale memory id(s) this one replaces.' }),
        ], { description: 'Stale memory id(s) this one replaces.' })),
        allow_similar: Type.Optional(Type.Boolean({ description: 'Bypass duplicate skip only for distinct new evidence.' })),
        failure_signature: optionalNonEmptyString(Type, 'Cluster key, e.g. mechanism:X|cause:Y.'),
      }),
    },
    {
      name: 'memory_reflect',
      type: 'reflect' as const,
      label: 'Memory: Reflect',
      description: 'Capture a reusable lesson after completing work. Prefer over memory_record when fix_repo, fix_harness, or failure_signature apply — those create refinements and cluster failure patterns automatically.',
      promptGuidelines: [
        'Prefer over memory_record when fix_repo, fix_harness, or failure_signature are relevant.',
        'Skip if there is no lesson, no failure pattern, and no repo/harness fix to propagate.',
      ],
      parameters: Type.Object({
        task: nonEmptyString(Type, 'Task just completed.'),
        outcome: Type.Optional(outcomeSchema),
        lesson: optionalNonEmptyString(Type, 'Durable reusable lesson; omit if none.'),
        worked: optionalNonEmptyString(Type, 'Concise note on what worked.'),
        didnt_work: optionalNonEmptyString(Type, 'Concise failure; used as lesson if lesson is omitted.'),
        fix_repo: optionalNonEmptyString(Type, 'Concrete repo-fix note; creates an open refinement.'),
        fix_harness: optionalNonEmptyString(Type, 'Harness/skill improvement; creates a harness-tagged memory.'),
        failure_signature: optionalNonEmptyString(Type, 'Cluster key, e.g. mechanism:X|cause:Y.'),
        importance: importanceSchema,
        judgment_note: optionalNonEmptyString(Type, 'Evidence checked + remaining uncertainty; folded into the reflection narrative.'),
        duo: Type.Optional(Type.Boolean({ description: 'Emit an advisory reflection_duo packet (supporter + skeptic prompts). Never stored.' })),
        eval_failures: Type.Optional(Type.Array(Type.Object({
          id: nonEmptyString(Type, 'Eval question/check id.'),
          dimension: optionalNonEmptyString(Type, 'Eval dimension, e.g. correctness.'),
          failure_signature: optionalNonEmptyString(Type, 'Cluster key for mine-weakness.'),
          suggested_lesson: optionalNonEmptyString(Type, 'Distilled lesson from the failed check.'),
        }), { description: 'Structured failed eval checks; each becomes an eval-tagged memory.' })),
        references: optionalStringArray(Type, 'Provenance such as file:/abs/path:line, pr:owner/repo#N, URL, npm:pkg@v.'),
        ...fileScopeProps,
        ...repoScopeProps,
        ...validityProps,
      }),
    },
    {
      name: 'workspace_status',
      type: 'workspace_status' as const,
      label: 'Workspace Status',
      description: 'Show active file locks, working agents, open signals/refinements, and memory store stats for the current workspace.',
      promptGuidelines: [
        'Use to check if another agent is editing files you need, or to see what is locked.',
        'Use before long edits to verify no conflicts exist.',
      ],
      parameters: workspaceStatusParams,
    },
    {
      name: 'agent_signal',
      type: 'agent_signal' as const,
      label: 'Agent Signal',
      description: 'Common agent coordination inbox: publish/list/reply/resolve questions, handoffs, blockers, decisions, and FYIs.',
      promptGuidelines: [
        'Use for agent-to-agent coordination: questions, replies, handoffs, blockers, decisions, FYIs.',
        'Use list to inspect unread signals; use reply/resolve to close loops instead of creating ad-hoc tools.',
        'This is an awareness inbox, not the source of truth for locks or verification.',
        'Use action:"ack" after processing a signal so hook delivery can safely replay until acknowledged.',
      ],
      parameters: Type.Object({
        action: agentSignalActionSchema,
        kind: Type.Optional(notificationKindSchema),
        subject: optionalNonEmptyString(Type, 'One-line signal subject for publish/reply.'),
        body: optionalNonEmptyString(Type, 'Optional detail.'),
        to_agents: optionalStringArray(Type, 'Recipient agent ids; omit/empty for broadcast.'),
        files: optionalStringArray(Type, 'Files this signal concerns.'),
        refs: optionalStringArray(Type, 'Related ids or references: memory ids, task ids, signal ids, URLs, PRs.'),
        importance: Type.Optional(Type.Integer({ minimum: 1, maximum: 10, description: 'Importance 1-10; default 5.' })),
        in_reply_to: optionalNonEmptyString(Type, 'Parent signal id for reply threading.'),
        thread_id: optionalNonEmptyString(Type, 'Thread id for list/resolve.'),
        signal_ids: optionalStringArray(Type, 'Signal ids for resolve/ack.'),
        unread_only: Type.Optional(Type.Boolean({ description: 'List only unread/open signals; default true.' })),
        mark_read: Type.Optional(Type.Boolean({ description: 'Mark listed signals as read.' })),
        kinds: Type.Optional(Type.Array(notificationKindSchema, { description: 'Filter list by signal kind.' })),
        limit: optionalLimit(Type, 'Max signals; default 20.'),
        agent_id: optionalNonEmptyString(Type, 'Agent id override; defaults to current Pi agent id.'),
        ...repoScopeProps,
      }),
    },
    {
      name: 'file_lock',
      type: 'file_lock' as const,
      label: 'File Lock',
      description: 'Manage file locks for parallel agents. type lock/release/status/renew; uses task_id as the safe release handle.',
      promptGuidelines: [
        'Prefer automatic edit/write locks; use this for explicit coordination across parallel agents.',
        'Release and renew by task_id whenever possible; agentId/sessionId are scope metadata, not precise lock handles.',
        'Set ttl_ms for bounded work; locks are capped by awareness to the maximum safe TTL.',
        'Include reasoning so status output explains why the files are locked.',
      ],
      parameters: fileLockParams,
    },
    {
      name: 'memory_workspace_status',
      type: 'workspace_status' as const,
      label: 'Memory: Workspace Status',
      description: 'Compatibility alias for workspace_status.',
      promptGuidelines: [
        'Prefer workspace_status for new usage; this alias is retained for compatibility.',
      ],
      parameters: workspaceStatusParams,
    },
    {
      name: 'memory_file_lock',
      type: 'file_lock' as const,
      label: 'Memory: File Lock',
      description: 'Compatibility alias for file_lock.',
      promptGuidelines: [
        'Prefer file_lock for new usage; this alias is retained for compatibility.',
      ],
      parameters: fileLockParams,
    },
    {
      name: 'memory_refine_get',
      type: 'refine_get' as const,
      label: 'Memory: Refinements',
      description: 'List open repo-fix refinements for the current workspace.',
      promptGuidelines: [
        'Use before related work when previous reflections may have left actionable fixes.',
        'Use memory_recall for broad prior lessons instead.',
      ],
      parameters: Type.Object({
        state: Type.Optional(refinementStateSchema),
        include_handoffs: Type.Optional(Type.Boolean({ description: 'Include session handoff rows; default false so repo-fix refinements stay visible.' })),
        limit: optionalLimit(Type, 'Max refinements; default 5.'),
        // getRefinements scopes by workspace + repo (no ref).
        ...workspaceScopeProp,
        repo: repoRefProps.repo,
      }),
    },
    {
      name: 'memory_audit_unverified',
      type: 'audit_unverified' as const,
      label: 'Memory: Audit Unverified',
      description: 'List pending edit tasks that still need verification. Auto-fires on agent_end; call manually only for a mid-turn check.',
      promptGuidelines: [
        'Run mid-turn when you suspect unverified edits; the agent_end hook already performs the final audit.',
        'If pending tasks exist, run the stated checks and clear with memory_verify({task_ids:[...], status}) for batch, memory_verify({allPending:true}) to clear all, or memory_verify({task_id, status}) for one.',
      ],
      parameters: Type.Object({}),
    },
    {
      name: 'memory_verify',
      type: 'verify' as const,
      label: 'Memory: Verify Task',
      description: 'Mark a pending edit task as verified or failed after running its check. Accepts a single task_id, a batch task_ids[] array, or allPending:true to clear every pending task for this agent in one call.',
      promptGuidelines: [
        'Use only after running the stated verification for the task.',
        'Never mark SUCCESS just to clear the gate.',
        'Prefer task_ids[] or allPending:true to clear multiple tasks in a single tool call instead of looping.',
      ],
      parameters: Type.Object({
        task_id: optionalNonEmptyString(Type, 'Single pending task id to verify.'),
        task_ids: Type.Optional(Type.Array(nonEmptyString(Type, 'Pending task id to verify.'), { minItems: 1, description: 'Batch: list of pending task ids to verify in one call.' })),
        allPending: Type.Optional(Type.Boolean({ description: 'Verify ALL pending tasks for this agent in one call. Pair with status.' })),
        status: Type.Optional(verifyStatusSchema),
      }),
    },
    {
      name: 'memory_export_harness',
      type: 'export_harness' as const,
      label: 'Memory: Export Harness',
      description: 'Export agent improvement proposals for AGENTS.md or CLAUDE.md. Tier 1: explicit harness proposals from memory_reflect fix_harness: (always first). Tier 2: high-importance general lessons. Raw reflections excluded. Never writes files — review and paste after human approval.',
      promptGuidelines: [
        'Never paste output into AGENTS.md without human review and explicit approval.',
        'Use harness_only:true to see only explicit fix_harness proposals, not general lessons.',
        'Route recurring failures through memory_reflect (fix_harness) first so this export has proposals to surface.',
      ],
      parameters: Type.Object({
        harness_only: Type.Optional(Type.Boolean({ description: 'Return only harness-tagged proposals (tier 1). Omit general lessons.' })),
        limit: optionalLimit(Type, 'Max memories; default 10.'),
        min_importance: Type.Optional(Type.Integer({ minimum: 1, maximum: 10, description: 'Minimum importance for tier 2 general lessons; default 7.' })),
        // exportHarness scopes by workspace only (no repo/ref upstream).
        ...workspaceScopeProp,
      }),
    },
    {
      name: 'memory_notify',
      type: 'notify' as const,
      label: 'Memory: Notify',
      description: 'Compatibility alias for agent_signal({action:"publish"}). Prefer agent_signal for list/reply/resolve.',
      promptGuidelines: [
        'Prefer agent_signal for new coordination; memory_notify only publishes a signal.',
        'Use for simple legacy handoffs/blockers/questions when no reply/list/resolve is needed.',
      ],
      parameters: Type.Object({
        kind: notificationKindSchema,
        subject: nonEmptyString(Type, 'One-line summary of the message.'),
        body: optionalNonEmptyString(Type, 'Optional detail.'),
        to_agent: optionalNonEmptyString(Type, 'Recipient agent id; omit to broadcast to all agents on this workspace.'),
        files: optionalStringArray(Type, 'Files this message concerns.'),
        importance: Type.Optional(Type.Integer({ minimum: 1, maximum: 10, description: 'Importance 1-10; default 5.' })),
        ...repoScopeProps,
      }),
    },
  ];

  for (const tool of tools) {
    registerMemoryTool(getAgentId, registerFn, pi, registeredToolNames, tool);
  }
}
