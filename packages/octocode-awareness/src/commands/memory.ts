import type { DatabaseSync } from 'node:sqlite';
import { insertMemoryWithSimilarityGate } from '../memory-write.js';
import { recallMemory, storeMemoryEmbeddingIfConfigured } from '../memory-semantic.js';
import type { MemoryRecord } from '../types/identity-memory.js';
import { normalizeFilePath, projectMemoryLean } from '../helpers.js';
import { MEMORY_SORTS, type ParsedArgs, resolveAgentId } from './args.js';
import { type EmitOptions, die, emit } from '../command-output.js';

export async function cmdTellMemory(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): Promise<number> {
  const agentId = resolveAgentId(args);
  const taskContext = String(args['task_context'] ?? '');
  const observation = String(args['observation'] ?? '');
  const importanceLevel = args['importance'];
  if (!taskContext) die('--task-context is required');
  if (!observation) die('--observation is required');
  const imp = parseInt(String(importanceLevel), 10);
  if (isNaN(imp) || imp < 1 || imp > 10) die('--importance must be 1–10');

  const rawTag = args['tag'];
  const tags = Array.isArray(rawTag) ? rawTag : rawTag ? [String(rawTag)] : [];
  const rawRef = args['reference'];
  const references = Array.isArray(rawRef) ? rawRef : rawRef ? [String(rawRef)] : [];
  const rawFile = args['file'];
  const files = Array.isArray(rawFile) ? rawFile : rawFile ? [String(rawFile)] : [];
  const workspaceForFiles = args['workspace'] ? String(args['workspace']) : undefined;
  const fileReferences = files
    .map((file) => {
      const trimmed = file.trim();
      if (!trimmed) return null;
      if (trimmed.startsWith('file:')) return trimmed;
      const normalized = normalizeFilePath(trimmed, workspaceForFiles);
      return normalized ? `file:${normalized}` : null;
    })
    .filter((file): file is string => Boolean(file));
  const rawSup = args['supersedes'];
  const supersedes = Array.isArray(rawSup) ? rawSup : rawSup ? [String(rawSup)] : [];
  const rawLabel = args['label'];
  const label = Array.isArray(rawLabel) ? rawLabel[0] : String(rawLabel ?? '');
  const guarded = await insertMemoryWithSimilarityGate(db, {
    agentId, taskContext, observation, importance: imp,
    label,
    tags, references: [...references, ...fileReferences], supersedes,
    failureSignature: args['failure_signature'] ? String(args['failure_signature']) : null,
    validFrom: args['valid_from'] ? String(args['valid_from']) : null,
    validTo: args['valid_to'] ? String(args['valid_to']) : null,
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    fileTreeFingerprint: args['file_tree_fingerprint'] ? String(args['file_tree_fingerprint']) : null,
    captureFingerprint: args['capture_fingerprint'] === true,
  }, Boolean(args['allow_similar']));

  if (guarded.skipped) {
    return emit({
      db_path: dbPath,
      skipped: true,
      reason: 'similar_memory_exists',
      similar: guarded.similar,
      next: 'Reuse the existing memory, supersede stale ids, or pass --allow-similar only for a materially distinct recurrence.',
    }, 0, opts);
  }
  const { memory, superseded, noveltyScore, similarMemoryIds } = guarded.result;
  const payload: Record<string, unknown> = { db_path: dbPath, memory, superseded };
  if (supersedes.length === 0 && noveltyScore < 0.5 && similarMemoryIds.length > 0) {
    payload['consolidation'] = {
      novelty_score: noveltyScore,
      similar_memory_ids: similarMemoryIds,
      hint: 'low novelty — review the similar memories; re-record with --supersedes <id> to replace one, or forget this one if redundant',
    };
  }
  const embeddingResult = storeMemoryEmbeddingIfConfigured(db, memory.memory_id, taskContext, observation);
  if (embeddingResult) payload['embedding'] = embeddingResult;
  return emit(payload, 0, opts);
}

export async function cmdGetMemory(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): Promise<number> {
  const rawLabel = args['label'];
  const labelArr = Array.isArray(rawLabel) ? rawLabel : rawLabel ? [String(rawLabel)] : undefined;
  const rawTag = args['tag'];
  const tags = Array.isArray(rawTag) ? rawTag : rawTag ? [String(rawTag)] : [];
  const rawState = args['state'];
  const states = rawState ? (Array.isArray(rawState) ? rawState : [String(rawState)]) : undefined;
  const rawReference = args['reference'];
  const references = Array.isArray(rawReference) ? rawReference : rawReference ? [String(rawReference)] : [];
  const rawRegex = args['regex'];
  const regex = Array.isArray(rawRegex) ? rawRegex : rawRegex ? [String(rawRegex)] : [];
  const rawFileRegex = args['file_regex'];
  const fileRegex = Array.isArray(rawFileRegex) ? rawFileRegex : rawFileRegex ? [String(rawFileRegex)] : [];
  const rawGetFiles = args['file'];
  const getFiles = Array.isArray(rawGetFiles) ? rawGetFiles : rawGetFiles ? [String(rawGetFiles)] : [];
  const sort = String(args['sort'] ?? 'smart');
  if (!MEMORY_SORTS.has(sort)) die(`--sort must be one of: ${[...MEMORY_SORTS].join(', ')}`);

  const payload: Record<string, unknown> = {
    db_path: dbPath,
    ...(await recallMemory(db, {
      query: String(args['query'] ?? ''),
      limit: parseInt(String(args['limit'] ?? '3'), 10),
      minImportance: parseInt(String(args['min_importance'] ?? '1'), 10),
      label: labelArr,
      tags,
      smart: args['smart'] === true || args['smart'] === 'true',
      workspacePath: args['workspace'] ? String(args['workspace']) : null,
      artifact: args['artifact'] ? String(args['artifact']) : null,
      repo: args['repo'] ? String(args['repo']) : null,
      ref: args['ref'] ? String(args['ref']) : null,
      states,
      sort,
      globalOnly: Boolean(args['global_only']),
      strictScope: Boolean(args['strict_scope']),
      allWorkspaces: Boolean(args['all_workspaces']),
      asOf: args['as_of'] ? String(args['as_of']) : null,
      references,
      regex,
      fileRegex,
      files: getFiles,
      explain: Boolean(args['explain']),
      checkFingerprint: args['check_fingerprint'] === true,
    }, Boolean(args['semantic']))),
  };
  if (opts.compact && payload['count'] === 0) {
    return emit({ count: 0, memories: [],
      ...(payload['partial'] ? { partial: true, partialReasons: payload['partialReasons'], terminalLimit: payload['terminalLimit'] } : {}),
    }, 0, opts);
  }
  if (!Boolean(args['full'])) {
    const memories = (payload['memories'] ?? []) as Array<Record<string, unknown>>;
    payload['memories'] = memories.map(memory => projectMemoryLean(memory as unknown as MemoryRecord));
    if (memories.length > 0) payload['projection'] = 'lean';
    if (payload['as_of'] == null) delete payload['as_of'];
    if (payload['global_only'] === false) delete payload['global_only'];
    if (payload['all_workspaces'] === false) delete payload['all_workspaces'];
    if (Array.isArray(payload['states']) && payload['states'].length === 1 && payload['states'][0] === 'ACTIVE') delete payload['states'];
  }
  return emit(payload, 0, opts);
}
