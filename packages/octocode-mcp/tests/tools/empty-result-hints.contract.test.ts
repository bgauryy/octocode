/**
 * Per-tool empty-result hint contract.
 *
 * For every remote tool, when a query returns zero results the response
 * MUST carry at least one query-aware recovery hint that:
 *   1. Names a concrete filter / parameter / value from the original query
 *      (no generic "try different keywords" prose).
 *   2. Proposes a single, executable next action (drop X, switch Y to Z).
 *   3. Avoids any banned static-guidance phrase.
 *
 * This file exercises each tool's `hints.ts` `empty` branch with realistic
 * query contexts and verifies the resulting strings against the policy.
 */

import { describe, it, expect } from 'vitest';
import { hints as codeHints } from '../../src/tools/github_search_code/hints.js';
import { hints as fetchHints } from '../../src/tools/github_fetch_content/hints.js';
import { hints as viewHints } from '../../src/tools/github_view_repo_structure/hints.js';
import { hints as prHints } from '../../src/tools/github_search_pull_requests/hints.js';

const BANNED = [
  'Got 3+ examples',
  "Use 'owner', 'repo'",
  'Try broader terms',
  'Final page',
  'Install: npm install',
  'Browse: https://',
];

function flatten(h: ReturnType<typeof codeHints.empty>): string[] {
  return h.filter((s): s is string => typeof s === 'string');
}

function assertActionable(line: string) {
  expect(line, `not banned: ${line}`).toEqual(
    expect.not.stringMatching(new RegExp(BANNED.join('|')))
  );
}

// ===========================================================================
// githubSearchCode — empty result paths
// ===========================================================================
describe('githubSearchCode.empty — query-aware recovery', () => {
  it('names match=path and points to match=file', () => {
    const h = flatten(codeHints.empty!({ match: 'path' }));
    expect(h.some(s => s.includes('match="file"'))).toBe(true);
    h.forEach(assertActionable);
  });

  it('names the scoped owner/repo when narrowed and empty', () => {
    const h = flatten(
      codeHints.empty!({
        hasOwnerRepo: true,
        owner: 'modelcontextprotocol',
        repo: 'typescript-sdk',
        extension: 'ts',
      })
    );
    expect(h.some(s => s.includes('modelcontextprotocol/typescript-sdk'))).toBe(
      true
    );
    expect(h.some(s => s.includes('extension'))).toBe(true);
  });

  it('calls out AND-logic when many keywords are combined', () => {
    const h = flatten(
      codeHints.empty!({
        hasOwnerRepo: false,
        keywords: ['parser', 'streaming', 'json', 'websocket'],
      })
    );
    expect(h.some(s => s.includes('4 keywords'))).toBe(true);
    expect(h.some(s => /AND/.test(s))).toBe(true);
  });

  it('flags over-restrictive multi-filter combinations', () => {
    const h = flatten(
      codeHints.empty!({
        hasOwnerRepo: true,
        owner: 'o',
        repo: 'r',
        extension: 'ts',
        filename: 'index',
        path: 'src',
      })
    );
    expect(h.some(s => s.includes('extension + filename + path'))).toBe(true);
  });

  it('emits cross-repo recovery when no owner/repo is set', () => {
    const h = flatten(codeHints.empty!({ hasOwnerRepo: false }));
    expect(h.some(s => s.includes('across repos'))).toBe(true);
    expect(h.some(s => /AND logic/.test(s))).toBe(true);
  });
});

// ===========================================================================
// githubGetFileContent — no logical empty state (not-found is an error)
// ===========================================================================
describe('githubGetFileContent.empty', () => {
  it('emits nothing — empty does not apply to single-file fetches', () => {
    expect(fetchHints.empty!({})).toEqual([]);
  });
});

// ===========================================================================
// githubViewRepoStructure — empty directory listings
// ===========================================================================
describe('githubViewRepoStructure.empty — query-aware recovery', () => {
  it('names the path and suggests depth=2 when depth=1 came back empty', () => {
    const h = flatten(viewHints.empty!({ path: 'packages/core', depth: 1 }));
    expect(h.some(s => s.includes("'packages/core'"))).toBe(true);
    expect(h.some(s => s.includes('depth=2'))).toBe(true);
  });

  it('names the branch when one was specified', () => {
    const h = flatten(
      viewHints.empty!({ path: 'src', branch: 'feat/x' } as Record<
        string,
        unknown
      >)
    );
    expect(h.some(s => s.includes("branch 'feat/x'"))).toBe(true);
  });

  it('falls back to "repository root" wording when path is absent', () => {
    const h = flatten(viewHints.empty!({}));
    expect(h.some(s => s.includes('repository root'))).toBe(true);
  });
});

// ===========================================================================
// githubSearchPullRequests — query-aware empty recovery
// ===========================================================================
describe('githubSearchPullRequests.empty — query-aware recovery', () => {
  it('names a missing PR number when one was looked up directly', () => {
    const h = flatten(
      prHints.empty!({ prNumber: 1234, owner: 'o', repo: 'r' } as Record<
        string,
        unknown
      >)
    );
    expect(h.some(s => s.includes('PR #1234'))).toBe(true);
    expect(h.some(s => s.includes('o/r'))).toBe(true);
  });

  it('suggests state="closed" when state="merged" came back empty', () => {
    const h = flatten(
      prHints.empty!({ state: 'merged', owner: 'o', repo: 'r' } as Record<
        string,
        unknown
      >)
    );
    expect(h.some(s => s.includes('state="closed"'))).toBe(true);
    expect(h.some(s => s.includes('o/r'))).toBe(true);
  });

  it('names the author filter when present', () => {
    const h = flatten(
      prHints.empty!({ author: 'octocat' } as Record<string, unknown>)
    );
    expect(h.some(s => s.includes('author=octocat'))).toBe(true);
  });

  it('quotes the query string when only a free-text search was used', () => {
    const h = flatten(
      prHints.empty!({ query: 'streaming refactor' } as Record<string, unknown>)
    );
    expect(h.some(s => s.includes('"streaming refactor"'))).toBe(true);
  });

  it('falls back to a generic "relax filters" line when nothing identifying is set', () => {
    const h = flatten(prHints.empty!({}));
    expect(h.length).toBeGreaterThan(0);
    expect(h[0]).toMatch(/Relax filters/);
  });
});

// ===========================================================================
// All-tools banned-phrase blanket check (defence in depth)
// ===========================================================================
describe('empty-result hints — banned-phrase blanket', () => {
  const samples: Array<[string, ReturnType<typeof codeHints.empty>]> = [
    ['code/path', codeHints.empty!({ match: 'path' })],
    [
      'code/narrowed',
      codeHints.empty!({
        hasOwnerRepo: true,
        owner: 'o',
        repo: 'r',
        extension: 'ts',
      }),
    ],
    ['code/cross', codeHints.empty!({ hasOwnerRepo: false })],
    ['view/path', viewHints.empty!({ path: 'src', depth: 1 })],
    [
      'pr/merged',
      prHints.empty!({ state: 'merged', owner: 'o', repo: 'r' } as Record<
        string,
        unknown
      >),
    ],
    ['pr/author', prHints.empty!({ author: 'a' } as Record<string, unknown>)],
  ];

  it.each(samples)('%s contains no banned phrase', (_label, hints) => {
    for (const hint of hints.filter(
      (s): s is string => typeof s === 'string'
    )) {
      for (const phrase of BANNED) {
        expect(hint).not.toContain(phrase);
      }
    }
  });
});
