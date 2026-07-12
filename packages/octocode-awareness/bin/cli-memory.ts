import type { DatabaseSync } from 'node:sqlite';
import { insertMemoryWithSimilarityGate, getMemory, storeEmbedding, searchByEmbedding, bumpAccess } from '../src/memory.js';
import { resolveEmbedCommand, runHostEmbedder } from '../src/embed-host.js';
import { insertRefinement, getRefinements, updateRefinement } from '../src/refinements.js';
import { reflect } from '../src/reflect.js';
import type { EvalFailure, MemoryRecord, RefinementQuality } from '../src/types.js';
import { normalizeFilePath, projectMemoryLean, summarizeText } from '../src/helpers.js';
import { MEMORY_SORTS, ParsedArgs } from './cli-model.js';
import { EmitOptions, die, emit, resolveAgentId, valuesFor } from './cli-routing.js';

export function cmdTellMemory(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
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
  const guarded = insertMemoryWithSimilarityGate(db, {
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

  // Consolidation surface (mem0 ADD/UPDATE/NOOP contract, LLM-free): when the
  // new memory overlaps existing ones, hand the calling agent the candidates
  // and let IT decide to supersede or forget — the store never guesses.
  const payload: Record<string, unknown> = { db_path: dbPath, memory, superseded };
  if (supersedes.length === 0 && noveltyScore < 0.5 && similarMemoryIds.length > 0) {
    payload['consolidation'] = {
      novelty_score: noveltyScore,
      similar_memory_ids: similarMemoryIds,
      hint: 'low novelty — review the similar memories; re-record with --supersedes <id> to replace one, or forget this one if redundant',
    };
  }
  const embedCmd = resolveEmbedCommand();
  if (embedCmd) {
    try {
      const text = `${taskContext}\n${observation}`.trim();
      const { embedding, model } = runHostEmbedder(text, { command: embedCmd });
      storeEmbedding(db, memory.memory_id, embedding, model);
      payload['embedding'] = { stored: true, model, dims: embedding.length };
    } catch (err) {
      payload['embedding'] = {
        stored: false,
        warning: err instanceof Error ? err.message : String(err),
      };
    }
  }
  return emit(payload, 0, opts);
}

export function cmdGetMemory(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
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
  if (!MEMORY_SORTS.has(sort)) {
    die(`--sort must be one of: ${[...MEMORY_SORTS].join(', ')}`);
  }

  const recallParams = {
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
    asOf: args['as_of'] ? String(args['as_of']) : null,
    references,
    regex,
    fileRegex,
    files: getFiles,
    explain: Boolean(args['explain']),
    recordAccess: !Boolean(args['semantic']),
  };
  // The lexical query is deferred: on a successful semantic run its result set
  // is fully replaced, so running it eagerly would be a discarded full FTS pass.
  const payload: Record<string, unknown> = { db_path: dbPath };
  if (args['semantic']) {
    const embedCmd = resolveEmbedCommand();
    const queryText = String(args['query'] ?? '').trim();
    if (!embedCmd) {
      payload['warnings'] = [
        'semantic ranking is unavailable in the CLI (set OCTOCODE_EMBED_CMD or use library storeEmbedding()/searchByEmbedding()); results use lexical FTS + decay.',
      ];
    } else if (!queryText) {
      payload['warnings'] = [
        'semantic ranking skipped: --query is required when OCTOCODE_EMBED_CMD is set; results use lexical FTS + decay.',
      ];
    } else {
      try {
        const { embedding, model } = runHostEmbedder(queryText, { command: embedCmd });
        const limit = parseInt(String(args['limit'] ?? '3'), 10);
        const semanticStates = states ?? (args['as_of'] ? ['ACTIVE', 'SUPERSEDED'] : ['ACTIVE']);
        // Rank the complete bounded embedding pool before final top-k. Applying
        // workspace/provenance filters after a global top-k can otherwise hide
        // valid in-scope results behind better out-of-scope matches.
        const hits = searchByEmbedding(db, embedding, 2_000, 0.0, model, semanticStates);
        if (hits.length === 0) {
          payload['warnings'] = [
            `OCTOCODE_EMBED_CMD ran (model=${model}) but no stored embeddings matched; results use lexical FTS + decay. Record memories while OCTOCODE_EMBED_CMD is set to populate vectors.`,
          ];
        } else {
          // Re-apply every normal recall filter (scope, temporal state,
          // provenance, file, regex, label, tags, importance) to the embedding
          // candidates, then re-rank the survivors by cosine similarity.
          const simById = new Map(hits.map(hit => [hit.memory_id, hit.similarity]));
          const scopedResult = getMemory(db, {
            ...recallParams,
            query: '',
            limit: hits.length,
            candidateMemoryIds: hits.map(hit => hit.memory_id),
            recordAccess: false,
            explain: false,
          });
          const ranked = scopedResult.memories
            .map(memory => {
              const similarity = simById.get(memory.memory_id) ?? 0;
              memory.score = similarity;
              memory.lexical = similarity;
              return memory;
            })
            .sort((a, b) => (b.score ?? 0) - (a.score ?? 0));
          if (ranked.length === 0) {
            payload['warnings'] = [
              `OCTOCODE_EMBED_CMD ran (model=${model}) and matched embeddings, but none passed the scope/label/importance filters; results use lexical FTS + decay.`,
            ];
          } else {
            bumpAccess(db, ranked.map(memory => memory.memory_id));
            Object.assign(payload, scopedResult);
            // Semantic mode: the candidate-scoped run's judgment fields describe
            // a lexical pass, not the semantic result set.
            delete payload['judgment_required'];
            delete payload['judgment_reason'];
            payload['memories'] = ranked.slice(0, limit);
            payload['count'] = Math.min(ranked.length, limit);
            payload['mode'] = 'semantic';
            payload['embedding_model'] = model;
          }
        }
      } catch (err) {
        payload['warnings'] = [
          `semantic ranking failed (${err instanceof Error ? err.message : String(err)}); results use lexical FTS + decay.`,
        ];
      }
    }
  }
  if (payload['mode'] !== 'semantic') {
    // Lexical run — the direct path without --semantic, and the fallback for
    // every non-success semantic branch above (warnings already in payload).
    Object.assign(payload, getMemory(db, recallParams));
  }
  if (args['semantic'] && payload['mode'] !== 'semantic') {
    const fallback = (payload['memories'] ?? []) as Array<{ memory_id?: string }>;
    bumpAccess(db, fallback.flatMap(memory => memory.memory_id ? [memory.memory_id] : []));
  }
  if (opts.compact && payload['count'] === 0) {
    return emit({ count: 0, memories: [] }, 0, opts);
  }
  if (!Boolean(args['full'])) {
    const memories = (payload['memories'] ?? []) as Array<Record<string, unknown>>;
    payload['memories'] = memories.map((memory) => projectMemoryLean(memory as unknown as MemoryRecord));
    if (memories.length > 0) payload['projection'] = 'lean';
    if (payload['as_of'] == null) delete payload['as_of'];
    if (payload['global_only'] === false) delete payload['global_only'];
    if (Array.isArray(payload['states']) && payload['states'].length === 1 && payload['states'][0] === 'ACTIVE') {
      delete payload['states'];
    }
  }
  return emit(payload, 0, opts);
}

export function cmdRefineSet(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawState = args['state'];
  const stateVal = Array.isArray(rawState) ? rawState[0] : String(rawState ?? 'open');
  const rawFile = args['file'];
  const files = Array.isArray(rawFile) ? rawFile : rawFile ? [String(rawFile)] : [];

  // Update path: --refinement-id changes only the passed fields
  // (open → ongoing → done lifecycle).
  const rawRefId = args['refinement_id'];
  const refinementId = Array.isArray(rawRefId) ? rawRefId[0] : rawRefId ? String(rawRefId) : null;
  if (refinementId && refinementId !== 'true') {
    const update = updateRefinement(db, {
      refinementId,
      ...(args['state'] !== undefined ? { state: stateVal as 'open' | 'ongoing' | 'done' } : {}),
      ...(args['quality'] !== undefined ? { quality: String(args['quality']) as RefinementQuality } : {}),
      ...(args['reasoning'] !== undefined ? { reasoning: String(args['reasoning']) } : {}),
      ...(args['remember'] !== undefined ? { remember: String(args['remember']) } : {}),
      ...(rawFile !== undefined ? { files } : {}),
      ...(args['state'] !== undefined && stateVal === 'done' ? {
        actorAgentId: resolveAgentId(args),
        checkReceipt: args['check_receipt'] ? String(args['check_receipt']) : '',
      } : {}),
    });
    if (!update.updated) die(`refinement not found: ${refinementId}`);
    return emit({ db_path: dbPath, updated: true, refinement: update.refinement }, 0, opts);
  }

  if (stateVal === 'done') {
    die('terminal refinement creation is not allowed; create open/ongoing, then close an existing --refinement-id with --agent-id and --check-receipt');
  }

  const reasoning = String(args['reasoning'] ?? '');
  const remember = String(args['remember'] ?? '');
  if (!reasoning) die('--reasoning is required');
  if (!remember) die('--remember is required');

  const { refinement } = insertRefinement(db, {
    agentId: resolveAgentId(args),
    reasoning, remember,
    quality: (String(args['quality'] ?? 'good')) as RefinementQuality,
    state: (stateVal ?? 'open') as 'open' | 'ongoing' | 'done',
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    files,
  });

  return emit({ db_path: dbPath, refinement }, 0, opts);
}

export function cmdRefineGet(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawState = args['state'];
  const states = rawState ? (Array.isArray(rawState) ? rawState : [String(rawState)]) : undefined;
  const full = Boolean(args['full']);
  const requestedLimit = parseInt(String(args['limit'] ?? (opts.compact && !full ? '3' : '10')), 10);

  const result = getRefinements(db, {
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    quality: args['quality'] ? String(args['quality']) as RefinementQuality : undefined,
    includeHandoffs: Boolean(args['include_handoffs']),
    states,
    limit: opts.compact && !full ? requestedLimit + 1 : requestedLimit,
  });

  if (opts.compact && !full) {
    const hasMore = result.refinements.length > requestedLimit;
    const refinements = result.refinements.slice(0, requestedLimit).map((row) => {
      const files = row.files.slice(0, 3);
      return {
        refinement_id: row.refinement_id,
        agent_id: row.agent_id,
        quality: row.quality,
        state: row.state,
        files,
        file_count: row.files.length,
        file_omitted_count: Math.max(0, row.files.length - files.length),
        reasoning_summary: summarizeText(row.reasoning, 120),
        remember_summary: summarizeText(row.remember, 160),
        updated_at: row.updated_at,
      };
    });
    return emit({
      db_path: dbPath,
      count: refinements.length,
      refinements,
      handoff_count: result.handoff_count,
      instructions_count: result.instructions_count,
      has_more: hasMore,
      next_limit: hasMore ? Math.min(50, requestedLimit * 2) : null,
    }, 0, opts);
  }
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

export function cmdReflect(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  if (!args['task']) die('--task is required');

  let evalFailures: EvalFailure[] = [];
  if (args['eval_failure_json']) {
    try {
      const parsed: unknown = JSON.parse(String(args['eval_failure_json']));
      if (!Array.isArray(parsed)) throw new Error('expected a JSON array');
      evalFailures = parsed as EvalFailure[];
    } catch (err) {
      die(`--eval-failure-json must be a JSON array of {id, dimension?, failure_signature?, suggested_lesson?}: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  const result = reflect(db, {
    agentId: resolveAgentId(args),
    task: String(args['task']),
    outcome: args['outcome'] != null ? String(args['outcome']) : 'partial',
    lesson: args['lesson'] ? String(args['lesson']) : null,
    worked: args['worked'] ? String(args['worked']) : null,
    didntWork: args['didnt_work'] ? String(args['didnt_work']) : null,
    fixRepo: args['fix_repo'] ? String(args['fix_repo']) : null,
    fixHarness: args['fix_harness'] ? String(args['fix_harness']) : null,
    fixInstructions: args['fix_instructions'] ? String(args['fix_instructions']) : null,
    failureSignature: args['failure_signature'] ? String(args['failure_signature']) : null,
    importance: args['importance'] ? parseInt(String(args['importance']), 10) : null,
    judgmentNote: args['judgment_note'] ? String(args['judgment_note']) : null,
    duo: Boolean(args['duo']),
    allowSimilar: Boolean(args['allow_similar']),
    evalFailures,
    files: valuesFor(args, 'fix_file'),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    cwd: args['workspace'] ? String(args['workspace']) : process.cwd(),
  });

  return emit({ ...result, db_path: dbPath }, 0, opts);
}
