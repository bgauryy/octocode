import type { DatabaseSync } from 'node:sqlite';
import { getMemory } from './memory-recall.js';
import { bumpAccess } from './memory-write.js';
import { searchByEmbedding, storeEmbedding } from './memory-embeddings.js';
import { resolveEmbedCommand, runHostEmbedder } from '@octocodeai/agent-contracts/embed';
import type { GetMemoryParams } from './types/identity-memory.js';

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
export function recallMemory(
  db: DatabaseSync,
  recallParams: GetMemoryParams,
  useSemantic: boolean,
): Record<string, unknown> {
  // Deferred access recording: when semantic mode is requested, the initial/
  // fallback getMemory call must not bump access itself — access is recorded
  // exactly once at the end, on whichever result set (semantic or fallback)
  // actually gets returned, to avoid double-counting recall hits.
  const baseParams: GetMemoryParams = useSemantic ? { ...recallParams, recordAccess: false } : recallParams;
  const payload: Record<string, unknown> = {};

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
          const simById = new Map(hits.map((hit) => [hit.memory_id, hit.similarity]));
          const scopedResult = getMemory(db, {
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
            bumpAccess(db, ranked.map((memory) => memory.memory_id));
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
    // Lexical run — the direct path without semantic, and the fallback for
    // every non-success semantic branch above (warnings already in payload).
    Object.assign(payload, getMemory(db, baseParams));
  }
  if (useSemantic && payload['mode'] !== 'semantic') {
    const fallback = (payload['memories'] ?? []) as Array<{ memory_id?: string }>;
    bumpAccess(db, fallback.flatMap((memory) => (memory.memory_id ? [memory.memory_id] : [])));
  }
  return payload;
}
