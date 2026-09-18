import type { DatabaseSync } from 'node:sqlite';
import { queryMemory, attachMemoryEvidence } from './memory-recall.js';
import { bumpAccess } from './memory-write.js';
import { searchByEmbedding, storeEmbedding, embeddingCandidateCount, EMBEDDING_CANDIDATE_LIMIT } from './memory-embeddings.js';
import { memoryRecallBounds } from './memory-limits.js';
import { resolveEmbedCommand, runHostEmbedder } from './embed.js';
import type { GetMemoryParams, GetMemoryResult } from './types/identity-memory.js';

export type StoreMemoryEmbeddingResult =
  | { stored: true; model: string; dims: number }
  | { stored: false; warning: string };

/**
 * Store an embedding for a freshly-recorded memory when OCTOCODE_EMBED_CMD is
 * configured; a no-op (returns undefined) when it isn't. Shared by the CLI
 * (`memory record`, which has always auto-embedded on write) and the MCP
 * `record` tool operation — semantic recall has nothing to rank without this,
 * so both write paths need to populate embeddings the same way.
 */
export function storeMemoryEmbeddingIfConfigured(
  db: DatabaseSync,
  memoryId: string,
  taskContext: string,
  observation: string,
): StoreMemoryEmbeddingResult | undefined {
  const embedCmd = resolveEmbedCommand();
  if (!embedCmd) return undefined;
  try {
    const text = `${taskContext}\n${observation}`.trim();
    const { embedding, model } = runHostEmbedder(text, { command: embedCmd });
    storeEmbedding(db, memoryId, embedding, model);
    return { stored: true, model, dims: embedding.length };
  } catch (err) {
    return { stored: false, warning: err instanceof Error ? err.message : String(err) };
  }
}

/**
 * Recall memories via getMemory, optionally ranked by embedding cosine
 * similarity instead of lexical FTS + decay. Shared by the CLI
 * (`memory recall --semantic`) and the MCP `recall` tool operation so both
 * surfaces behave identically instead of drifting.
 *
 * Semantic mode requires OCTOCODE_EMBED_CMD (an external embedding command)
 * and a non-empty query; any missing prerequisite, embedder failure, or
 * empty/filtered-out result set degrades gracefully to the lexical result
 * with an explanatory warning in `warnings` — requesting semantic mode never
 * throws or returns an error, only ever a lexical fallback plus a reason.
 */
export async function recallMemory(
  db: DatabaseSync,
  recallParams: GetMemoryParams,
  useSemantic: boolean,
): Promise<Record<string, unknown>> {
  const { checkFingerprint: _checkFingerprint, ...queryParams } = recallParams;
  const result = queryMemorySemantic(db, queryParams, useSemantic);
  if (recallParams.checkFingerprint) await attachMemoryEvidence(result['memories'] as GetMemoryResult['memories'], recallParams);
  return result;
}

/** SQL/embedding selection only; explicit evidence validation belongs to recallMemory. */
export function queryMemorySemantic(
  db: DatabaseSync,
  recallParams: Omit<GetMemoryParams, 'checkFingerprint'>,
  useSemantic: boolean,
): Record<string, unknown> {
  if ('checkFingerprint' in recallParams && recallParams.checkFingerprint) throw new Error('Use recallMemory for filesystem evidence checks');
  // Deferred access recording: when semantic mode is requested, the initial/
  // fallback getMemory call must not bump access itself — access is recorded
  // exactly once at the end, on whichever result set (semantic or fallback)
  // actually gets returned, to avoid double-counting recall hits.
  const baseParams: GetMemoryParams = useSemantic ? { ...recallParams, recordAccess: false } : recallParams;
  const payload: Record<string, unknown> = {};
  let semanticCandidateLimited = false;

  if (useSemantic) {
    const embedCmd = resolveEmbedCommand();
    const queryText = String(recallParams.query ?? '').trim();
    if (!embedCmd) {
      payload['warnings'] = [
        'semantic ranking is unavailable (set OCTOCODE_EMBED_CMD or use library storeEmbedding()/searchByEmbedding()); results use lexical FTS + decay.',
      ];
    } else if (!queryText) {
      payload['warnings'] = [
        'semantic ranking skipped: a query is required when OCTOCODE_EMBED_CMD is set; results use lexical FTS + decay.',
      ];
    } else {
      try {
        const { embedding, model } = runHostEmbedder(queryText, { command: embedCmd });
        const limit = Math.max(1, Number(recallParams.limit ?? 3) || 3);
        const semanticStates = recallParams.states ?? (recallParams.asOf ? ['ACTIVE', 'SUPERSEDED'] : ['ACTIVE']);
        semanticCandidateLimited = embeddingCandidateCount(db, model, semanticStates) > EMBEDDING_CANDIDATE_LIMIT;
        // Rank the complete bounded embedding pool before final top-k. Applying
        // workspace/provenance filters after a global top-k can otherwise hide
        // valid in-scope results behind better out-of-scope matches.
        const hits = searchByEmbedding(db, embedding, EMBEDDING_CANDIDATE_LIMIT, 0.0, model, semanticStates);
        if (hits.length === 0) {
          payload['warnings'] = [
            `OCTOCODE_EMBED_CMD ran (model=${model}) but no stored embeddings matched; results use lexical FTS + decay. Record memories while OCTOCODE_EMBED_CMD is set to populate vectors.`,
          ];
        } else {
          // Re-apply every normal recall filter (scope, temporal state,
          // provenance, file, regex, label, tags, importance) to the embedding
          // candidates, then re-rank the survivors by cosine similarity.
          const simById = new Map(hits.map((hit) => [hit.memory_id, hit.similarity]));
          const scopedResult = queryMemory(db, {
            ...baseParams,
            query: '',
            limit: hits.length,
            candidateMemoryIds: hits.map((hit) => hit.memory_id),
            recordAccess: false,
            explain: false,
          });
          const ranked = scopedResult.memories
            .map((memory) => {
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
            const returned = ranked.slice(0, limit);
            if (recallParams.recordAccess !== false) {
              bumpAccess(db, returned.map((memory) => memory.memory_id));
            }
            Object.assign(payload, scopedResult);
            // Semantic mode: the candidate-scoped run's judgment fields describe
            // a lexical pass, not the semantic result set.
            delete payload['judgment_required'];
            delete payload['judgment_reason'];
            payload['memories'] = returned;
            payload['count'] = returned.length;
            payload['mode'] = 'semantic';
            payload['embedding_model'] = model;
            Object.assign(payload, memoryRecallBounds(semanticCandidateLimited, ranked.length > limit, EMBEDDING_CANDIDATE_LIMIT, limit));
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
    // Lexical run — the direct path without semantic, and the fallback for
    // every non-success semantic branch above (warnings already in payload).
    Object.assign(payload, queryMemory(db, baseParams));
    if (semanticCandidateLimited) Object.assign(payload, memoryRecallBounds(true, payload['partial'] === true, EMBEDDING_CANDIDATE_LIMIT, Number(recallParams.limit ?? 3)));
  }
  if (useSemantic && payload['mode'] !== 'semantic' && recallParams.recordAccess !== false) {
    const fallback = (payload['memories'] ?? []) as Array<{ memory_id?: string }>;
    bumpAccess(db, fallback.flatMap((memory) => (memory.memory_id ? [memory.memory_id] : [])));
  }
  return payload;
}
