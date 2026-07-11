import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact } from './helpers.js';
import { normalizeWorkspacePath } from './git.js';
import { jaccard } from './memory-scoring.js';

// ─── mineWeakness ─────────────────────────────────────────────────────────────

export interface WeaknessCluster {
  failure_signature: string;  // raw (may include |surface:Z suffix)
  base_signature: string;     // without |surface:Z — use this for display/grouping
  surfaces: string[];         // extracted surface values across all merged signatures
  count: number;
  avg_importance: number;
  score: number;
  memory_ids: string[];
  representative: string;
  labels: string[];
}

export interface MineWeaknessResult {
  ok: true;
  clusters: WeaknessCluster[];
  total_signatures: number;
  total_memories: number;
  next: string;
}

export interface MineWeaknessParams {
  agentId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  minCount?: number;
  limit?: number;
  cwd?: string;
}

/** Strip optional |surface:Z suffix from a failure_signature for cluster merging. */
export function stripSurface(sig: string): string {
  const idx = sig.indexOf('|surface:');
  return idx >= 0 ? sig.slice(0, idx) : sig;
}

/** Extract the |surface:Z value if present. */
export function extractSurface(sig: string): string | null {
  const idx = sig.indexOf('|surface:');
  return idx >= 0 ? sig.slice(idx + 9) : null;
}

/** Tokenize a failure_signature for Jaccard similarity (splits on |:). */
export function sigTokens(sig: string): Set<string> {
  return new Set(
    sig.split(/[|:]+/)
      .map(s => s.trim().toLowerCase())
      .filter(s => s.length > 2 && s !== 'mechanism' && s !== 'cause' && s !== 'surface'),
  );
}

/**
 * Cluster memories by failure_signature to surface recurring failure patterns.
 * Sorted by count × avg_importance so the most impactful patterns appear first.
 *
 * Improvements vs naive GROUP BY:
 * 1. Signatures differing only in |surface:Z suffix are merged into one cluster
 *    (base_signature is the cluster key; surfaces[] collects all variants).
 * 2. Diversity filter: a cluster is suppressed if Jaccard similarity ≥ 0.5 vs
 *    any already-selected cluster, so the output covers distinct failure mechanisms
 *    rather than N variants of the same one.
 */
export function mineWeakness(db: DatabaseSync, params: MineWeaknessParams = {}): MineWeaknessResult {
  const { minCount = 2, limit = 20, cwd } = params;
  const wsPath = params.workspacePath
    ? normalizeWorkspacePath(params.workspacePath, params.workspacePath)
    : (cwd ? normalizeWorkspacePath(null, cwd) : null);
  const artifact = normalizeArtifact(params.artifact);

  const conditions: string[] = ["failure_signature IS NOT NULL", "state = 'ACTIVE'"];
  const bindParams: (string | number)[] = [];
  if (wsPath) { conditions.push('(workspace_path = ? OR workspace_path IS NULL)'); bindParams.push(wsPath); }
  if (artifact) { conditions.push('(artifact = ? OR artifact IS NULL)'); bindParams.push(artifact); }
  if (params.agentId) { conditions.push('agent_id = ?'); bindParams.push(params.agentId); }

  type ClusterRow = { failure_signature: string; freq: number; avg_imp: number; score: number; ids: string; labels: string };
  const rows = db.prepare(`
    SELECT failure_signature,
           count(*) AS freq,
           avg(importance) AS avg_imp,
           count(*) * avg(importance) AS score,
           group_concat(memory_id, ',') AS ids,
           group_concat(DISTINCT label) AS labels
    FROM memories
    WHERE ${conditions.join(' AND ')}
    GROUP BY failure_signature
    ORDER BY score DESC
  `).all(...bindParams) as unknown as ClusterRow[];

  // Phase 1: merge rows that share the same base_signature (differ only in |surface:Z).
  // Key = stripped signature; value = merged cluster accumulators.
  interface Merged {
    base_sig: string;
    raw_sig: string;    // highest-score raw signature (for rep lookup)
    total_freq: number;
    total_score: number;
    importance_sum: number;
    ids: string[];
    labels: Set<string>;
    surfaces: Set<string>;
    raw_score: number;
  }
  const mergedMap = new Map<string, Merged>();
  for (const row of rows) {
    const base = stripSurface(row.failure_signature);
    const surface = extractSurface(row.failure_signature);
    const existing = mergedMap.get(base);
    if (existing) {
      existing.total_freq += row.freq;
      existing.total_score += row.score;
      existing.importance_sum += row.avg_imp * row.freq;
      existing.ids.push(...row.ids.split(','));
      for (const l of row.labels.split(',').filter(Boolean)) existing.labels.add(l);
      if (surface) existing.surfaces.add(surface);
      // Keep the raw signature with the highest original score for rep lookup
      if (row.score > existing.raw_score) {
        existing.raw_sig = row.failure_signature;
        existing.raw_score = row.score;
      }
    } else {
      mergedMap.set(base, {
        base_sig: base,
        raw_sig: row.failure_signature,
        total_freq: row.freq,
        total_score: row.score,
        importance_sum: row.avg_imp * row.freq,
        ids: row.ids.split(','),
        labels: new Set(row.labels.split(',').filter(Boolean)),
        surfaces: new Set(surface ? [surface] : []),
        raw_score: row.score,
      });
    }
  }

  // Re-sort merged clusters by total_score DESC.
  const merged = [...mergedMap.values()]
    .filter(m => m.total_freq >= minCount)
    .sort((a, b) => b.total_score - a.total_score);

  // Phase 2: batch-fetch representatives for all distinct base signatures.
  type RepRow = { failure_signature: string; observation: string };
  const repMap = new Map<string, string>();
  const allRawSigs = merged.map(m => m.raw_sig);
  if (allRawSigs.length > 0) {
    const ph = allRawSigs.map(() => '?').join(',');
    const repRows = db.prepare(
      `SELECT failure_signature, observation, max(importance)
       FROM memories
       WHERE failure_signature IN (${ph}) AND ${conditions.join(' AND ')}
       GROUP BY failure_signature`
    ).all(...allRawSigs, ...bindParams) as unknown as RepRow[];
    for (const r of repRows) repMap.set(stripSurface(r.failure_signature), r.observation);
  }

  // Phase 3: Jaccard diversity filter — skip cluster if ≥ 0.5 overlap with any already-selected.
  const selected: WeaknessCluster[] = [];
  for (const m of merged) {
    if (selected.length >= limit) break;
    const toks = sigTokens(m.base_sig);
    // FIX #10: use single jaccard() function (jaccardSimilarity removed as redundant duplicate).
    const tooSimilar = selected.some(
      sel => jaccard(sigTokens(sel.base_signature), toks) >= 0.5,
    );
    if (tooSimilar) continue;
    selected.push({
      failure_signature: m.raw_sig,
      base_signature: m.base_sig,
      surfaces: [...m.surfaces].sort(),
      count: m.total_freq,
      avg_importance: Math.round((m.importance_sum / m.total_freq) * 10) / 10,
      score: Math.round(m.total_score * 10) / 10,
      memory_ids: [...new Set(m.ids)],
      representative: (repMap.get(m.base_sig) ?? '').slice(0, 200),
      labels: [...m.labels].sort(),
    });
  }

  type TotalRow = { sigs: number; mems: number };
  const totals = db.prepare(
    `SELECT count(DISTINCT failure_signature) AS sigs, count(*) AS mems
     FROM memories WHERE ${conditions.join(' AND ')}`
  ).get(...bindParams) as unknown as TotalRow;

  const next = selected.length > 0
    ? 'Next: choose one cluster, inspect its memory_ids, implement one scoped fix, verify it, then run octocode-awareness reflect record with the same --failure-signature and either --fix-repo or --fix-harness.'
    : 'No recurring failure cluster met the threshold. Record verified failures with octocode-awareness reflect record --failure-signature <signature>, then mine again after repetition.';

  return { ok: true, clusters: selected, total_signatures: totals.sigs, total_memories: totals.mems, next };
}
