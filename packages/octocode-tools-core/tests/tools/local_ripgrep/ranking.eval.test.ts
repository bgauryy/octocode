/**
 * Golden-fixture ranking eval (Tier 1 Phase 4) — a regression guard, not a
 * one-shot check. Runs the REAL search pipeline (native engine -> executor ->
 * relevance ranker) against this package's own src/ and measures whether the
 * agent's goal is met: the file that DEFINES a searched symbol should rank
 * first. Compares sort:"relevance" against the legacy sort:"matchCount".
 *
 * Robustness: labels are basename substrings and the per-query assertion is
 * top-3 (a refactor that moves a symbol keeps it findable). The hard guard is
 * comparative — relevance must never do worse than matchCount on top-1 — which
 * tests the RANKER, not the exact repo layout.
 */
import { describe, expect, it } from 'vitest';
import { join } from 'node:path';

import { executeRipgrepSearchInternal } from '../../../src/tools/local_ripgrep/ripgrepExecutor.js';

const SRC = join(process.cwd(), 'src');

/**
 * Hand-labeled fixtures: a symbol query and the basename of the file that
 * defines it (verified against the live tree). These are unambiguous "owning
 * file" cases — exactly the most common agent grep ("find where X lives").
 */
const FIXTURES: Array<{ query: string; defFile: string }> = [
  { query: 'scoreFile', defFile: 'rankingScoring.ts' },
  { query: 'classifyPathRole', defFile: 'rankingProfiles.ts' },
  { query: 'buildSearchNextMap', defFile: 'searchNext.ts' },
  { query: 'buildSearchResult', defFile: 'buildResult.ts' },
  { query: 'validateToolPath', defFile: 'toolHelpers.ts' },
  { query: 'executeRipgrepSearchInternal', defFile: 'ripgrepExecutor.ts' },
];

type Sort = 'relevance' | 'matchCount';

async function rankedPaths(query: string, sort: Sort): Promise<string[]> {
  const res = (await executeRipgrepSearchInternal({
    keywords: query,
    path: SRC,
    sort,
    itemsPerPage: 50,
    maxMatchesPerFile: 20,
  } as never)) as { files?: Array<{ path: string }> };
  return (res.files ?? []).map(f => f.path);
}

function rankOf(paths: string[], defFile: string): number {
  const i = paths.findIndex(
    p => p.endsWith('/' + defFile) || p.endsWith(defFile)
  );
  return i < 0 ? Infinity : i + 1; // 1-based; Infinity = not found
}

describe('ranking eval — definition-first hit rate (relevance vs matchCount)', () => {
  it('relevance ranks the defining file in the top 3 for every fixture', async () => {
    const misses: string[] = [];
    for (const { query, defFile } of FIXTURES) {
      const rank = rankOf(await rankedPaths(query, 'relevance'), defFile);
      if (rank > 3) misses.push(`${query} -> ${defFile} (rank ${rank})`);
    }
    expect(misses, `top-3 misses: ${misses.join('; ')}`).toEqual([]);
  });

  it('relevance is never worse than matchCount at top-1 (regression guard)', async () => {
    let relTop1 = 0;
    let mcTop1 = 0;
    const rows: string[] = [];
    for (const { query, defFile } of FIXTURES) {
      const relRank = rankOf(await rankedPaths(query, 'relevance'), defFile);
      const mcRank = rankOf(await rankedPaths(query, 'matchCount'), defFile);
      if (relRank === 1) relTop1++;
      if (mcRank === 1) mcTop1++;
      rows.push(
        `${query.padEnd(30)} relevance#${relRank}  matchCount#${mcRank}`
      );
    }
    // Visible summary for humans reading CI output.

    console.log(
      `\n[ranking eval] top-1 definition hit rate — relevance ${relTop1}/${FIXTURES.length}, matchCount ${mcTop1}/${FIXTURES.length}\n` +
        rows.join('\n')
    );
    expect(relTop1).toBeGreaterThanOrEqual(mcTop1);
    // Floor: relevance should nail the majority outright.
    expect(relTop1 / FIXTURES.length).toBeGreaterThanOrEqual(0.7);
  });
});

describe('ranking eval — agent off-switch', () => {
  it('sort:"matchCount" disables classification and ranking metadata', async () => {
    const res = (await executeRipgrepSearchInternal({
      keywords: 'fallback',
      path: SRC,
      sort: 'matchCount',
      debugRanking: true, // even when explicitly asked, off means off
      itemsPerPage: 5,
    } as never)) as {
      files?: Array<{
        matchCount?: number;
        matches?: Array<{ kind?: string }>;
        ranking?: unknown;
      }>;
    };
    const files = res.files ?? [];
    expect(files.length).toBeGreaterThan(0);
    // No AST classification cost was paid.
    const classified = files.some(f =>
      (f.matches ?? []).some(m => m.kind !== undefined)
    );
    expect(classified).toBe(false);
    // No ranking metadata attached.
    expect(files.some(f => f.ranking !== undefined)).toBe(false);
    // Legacy behavior: count-first, non-increasing matchCount.
    const counts = files.map(f => f.matchCount ?? 0);
    expect(counts).toEqual([...counts].sort((a, b) => b - a));
  });

  it('sort:"path" gives deterministic alphabetical order with no classification', async () => {
    const res = (await executeRipgrepSearchInternal({
      keywords: 'fallback',
      path: SRC,
      sort: 'path',
      itemsPerPage: 5,
    } as never)) as {
      files?: Array<{ path: string; matches?: Array<{ kind?: string }> }>;
    };
    const files = res.files ?? [];
    const paths = files.map(f => f.path);
    expect(paths).toEqual([...paths].sort((a, b) => a.localeCompare(b)));
    expect(
      files.some(f => (f.matches ?? []).some(m => m.kind !== undefined))
    ).toBe(false);
  });
});
