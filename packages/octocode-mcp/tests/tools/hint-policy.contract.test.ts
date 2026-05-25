/**
 * Hint policy contract — single source of truth for "what hints are allowed
 * to appear in a tool response under the new strict policy."
 *
 * The policy:
 *   1. Pagination cursor — only when more pages exist; one line with
 *      `Page N/M (...)` and the concrete `Next: <param>=N+1` cursor.
 *   2. Recovery directive — only on failure (rate limit, auth, size,
 *      empty-with-context) and must reference a concrete next action.
 *   3. Nothing else. No data echo (Install/Explore/Browse/owner=...),
 *      no narration (Final page / Complete content retrieved / token noise),
 *      no static guidance (Got 3+ examples / Use 'owner','repo').
 *
 * Each generator below is exercised across its `hasMore=true` and
 * `hasMore=false` shape so the policy is enforced everywhere hints are born.
 */

import { describe, it, expect } from 'vitest';
import {
  generatePaginationHints,
  generateGitHubPaginationHints,
  generateStructurePaginationHints,
} from '../../src/utils/pagination/hints.js';
import { buildPaginationHints } from '../../src/tools/providerMappers.js';
import { hints as packageHints } from '../../src/tools/package_search/hints.js';
import { hints as codeHints } from '../../src/tools/github_search_code/hints.js';
import { hints as fetchHints } from '../../src/tools/github_fetch_content/hints.js';

// ---------------------------------------------------------------------------
// Phrases that must NEVER appear in any hint under the new policy.
// ---------------------------------------------------------------------------
const BANNED_PHRASES = [
  'Final page',
  'Complete content retrieved',
  'Complete structure retrieved',
  'TO GET NEXT PAGE',
  'Same params:',
  '💡 TIP',
  '📂',
  '📊',
  '▶',
  '✓ Efficient',
  '✓ Final',
  'ℹ️ Moderate',
  'NOTICE',
  'Install: npm install',
  'Install: pip install',
  'Explore: githubViewRepoStructure',
  'Browse: https://',
  'Compare: Check weeklyDownloads',
  "Use 'owner', 'repo'",
  "Follow 'mainResearchGoal'",
  'Got 3+ examples',
  'Do findings answer your question',
];

function assertNoBannedPhrases(hints: readonly string[]): void {
  for (const hint of hints) {
    for (const phrase of BANNED_PHRASES) {
      expect(hint, `banned phrase "${phrase}" in: ${hint}`).not.toContain(
        phrase
      );
    }
  }
}

// ===========================================================================
// generic char-based pagination
// ===========================================================================
describe('generatePaginationHints — strict cursor/recovery policy', () => {
  it('emits a single cursor when more pages exist', () => {
    const hints = generatePaginationHints({
      paginatedContent: 'x',
      charOffset: 0,
      charLength: 10,
      totalChars: 30,
      byteOffset: 0,
      byteLength: 10,
      totalBytes: 30,
      hasMore: true,
      nextCharOffset: 10,
      currentPage: 1,
      totalPages: 3,
    });
    expect(hints).toHaveLength(1);
    expect(hints[0]).toMatch(/Page 1\/3/);
    expect(hints[0]).toContain('charOffset=10');
    assertNoBannedPhrases(hints);
  });

  it('emits NO hint on the final page', () => {
    const hints = generatePaginationHints({
      paginatedContent: 'x',
      charOffset: 0,
      charLength: 10,
      totalChars: 10,
      byteOffset: 0,
      byteLength: 10,
      totalBytes: 10,
      hasMore: false,
      currentPage: 1,
      totalPages: 1,
    });
    expect(hints).toEqual([]);
  });

  it('surfaces a token-budget recovery directive only above 30K tokens', () => {
    const fifty = generatePaginationHints({
      paginatedContent: 'x',
      charOffset: 0,
      charLength: 1,
      totalChars: 1,
      byteOffset: 0,
      byteLength: 1,
      totalBytes: 1,
      hasMore: false,
      currentPage: 1,
      totalPages: 1,
      estimatedTokens: 55000,
    });
    expect(fifty.some(h => /exceeds typical context/.test(h))).toBe(true);

    const moderate = generatePaginationHints({
      paginatedContent: 'x',
      charOffset: 0,
      charLength: 1,
      totalChars: 1,
      byteOffset: 0,
      byteLength: 1,
      totalBytes: 1,
      hasMore: false,
      currentPage: 1,
      totalPages: 1,
      estimatedTokens: 25000,
    });
    expect(moderate).toEqual([]);
  });
});

// ===========================================================================
// GitHub file-content pagination
// ===========================================================================
describe('generateGitHubPaginationHints — strict cursor policy', () => {
  it('emits exactly one cursor line when hasMore=true', () => {
    const hints = generateGitHubPaginationHints(
      {
        currentPage: 1,
        totalPages: 3,
        hasMore: true,
        byteOffset: 0,
        byteLength: 20000,
        totalBytes: 60000,
      },
      { owner: 'o', repo: 'r', path: 'a.ts', branch: 'main' }
    );
    expect(hints).toHaveLength(1);
    expect(hints[0]).toContain('Page 1/3');
    expect(hints[0]).toContain('charOffset=20000');
    assertNoBannedPhrases(hints);
    // No param echo — owner/repo/path/branch live on the caller's query.
    expect(hints[0]).not.toContain('owner=');
    expect(hints[0]).not.toContain('repo=');
    expect(hints[0]).not.toContain('branch=');
    expect(hints[0]).not.toContain('path=');
  });

  it('emits NO hint when hasMore=false', () => {
    expect(
      generateGitHubPaginationHints(
        {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          byteOffset: 0,
          byteLength: 100,
          totalBytes: 100,
        },
        { owner: 'o', repo: 'r', path: 'a.ts' }
      )
    ).toEqual([]);
  });
});

// ===========================================================================
// Repo structure pagination
// ===========================================================================
describe('generateStructurePaginationHints — strict cursor policy', () => {
  it('emits one cursor line when hasMore=true; no file/folder narration', () => {
    const hints = generateStructurePaginationHints(
      {
        currentPage: 1,
        totalPages: 3,
        hasMore: true,
        entriesPerPage: 20,
        totalEntries: 55,
      },
      {
        owner: 'o',
        repo: 'r',
        branch: 'main',
        path: 'src',
        depth: 2,
        pageFiles: 15,
        pageFolders: 5,
        allFiles: 40,
        allFolders: 15,
      }
    );
    expect(hints).toHaveLength(1);
    expect(hints[0]).toContain('Page 1/3');
    expect(hints[0]).toContain('entryPageNumber=2');
    assertNoBannedPhrases(hints);
    expect(hints[0]).not.toMatch(/files/);
    expect(hints[0]).not.toMatch(/folders/);
  });

  it('emits NO hint when hasMore=false (no tautology)', () => {
    expect(
      generateStructurePaginationHints(
        {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          entriesPerPage: 50,
          totalEntries: 35,
        },
        {
          owner: 'o',
          repo: 'r',
          branch: 'main',
          pageFiles: 30,
          pageFolders: 5,
          allFiles: 30,
          allFolders: 5,
        }
      )
    ).toEqual([]);
  });
});

// ===========================================================================
// providerMappers.buildPaginationHints — used by search-* tools
// ===========================================================================
describe('buildPaginationHints — strict cursor policy', () => {
  it('emits one combined Page+Next line when hasMore=true', () => {
    const hints = buildPaginationHints(
      {
        currentPage: 2,
        totalPages: 5,
        hasMore: true,
        totalMatches: 50,
        perPage: 10,
      },
      'matches'
    );
    expect(hints).toHaveLength(1);
    expect(hints[0]).toContain('Page 2/5');
    expect(hints[0]).toContain('Next: page=3');
    assertNoBannedPhrases(hints);
  });

  it('emits NO hint on final page regardless of totalPages', () => {
    expect(
      buildPaginationHints(
        {
          currentPage: 5,
          totalPages: 5,
          hasMore: false,
          totalMatches: 50,
          perPage: 10,
        },
        'matches'
      )
    ).toEqual([]);
  });
});

// ===========================================================================
// packageSearch hints — deprecation recovery only; no data-echo
// ===========================================================================
describe('packageSearch hints — recovery-only contract', () => {
  it('hasResults: emits no hint by default (no Install/Explore/Browse echo)', () => {
    expect(packageHints.hasResults!({})).toEqual([]);
  });

  it('empty: emits a recovery line plus name-variant suggestions; no Browse URL', () => {
    // Empty hints are generated inside execution.ts (which has the query),
    // not the hints.ts registry. The registry's empty branch stays minimal.
    expect(packageHints.empty!({})).toEqual([]);
  });

  it('error: emits nothing without context (no static guidance)', () => {
    expect(packageHints.error!({})).toEqual([]);
  });
});

// ===========================================================================
// githubSearchCode hints — empty-with-context + auth/rate-limit recovery
// ===========================================================================
describe('githubSearchCode hints — recovery-only', () => {
  it('hasResults: always empty (no narration)', () => {
    expect(codeHints.hasResults!({ hasOwnerRepo: true })).toEqual([]);
    expect(codeHints.hasResults!({ hasOwnerRepo: false })).toEqual([]);
  });

  it('empty + match=path: emits actionable redirect', () => {
    const h = codeHints.empty!({ match: 'path' });
    expect(h.some(s => s?.includes('match="file"'))).toBe(true);
    assertNoBannedPhrases(h.filter((x): x is string => !!x));
  });

  it('empty + no owner/repo: emits cross-repo recovery line', () => {
    const h = codeHints.empty!({ hasOwnerRepo: false });
    expect(h.some(s => s?.includes('across repos'))).toBe(true);
  });

  it('error + rate-limit: emits Retry-after directive', () => {
    const h = codeHints.error!({ isRateLimited: true, retryAfter: 30 });
    expect(h.some(s => s?.includes('Retry after 30s'))).toBe(true);
  });

  it('error + 401: emits provider-specific token guidance', () => {
    const h = codeHints.error!({ status: 401 });
    expect(h.some(s => s?.includes('GITHUB_TOKEN'))).toBe(true);
  });
});

// ===========================================================================
// githubGetFileContent hints — partial-content cursor + size recovery
// ===========================================================================
describe('githubGetFileContent hints — recovery-only', () => {
  it('hasResults + isPartial: emits continuation cursor', () => {
    const h = fetchHints.hasResults!({
      isPartial: true,
      endLine: 80,
    } as Record<string, unknown>);
    expect(h.some(s => s?.includes('startLine=81'))).toBe(true);
  });

  it('hasResults without isPartial: emits nothing', () => {
    expect(fetchHints.hasResults!({})).toEqual([]);
  });

  it('error + size_limit: emits matchString / line-range recovery', () => {
    const h = fetchHints.error!({ errorType: 'size_limit', fileSize: 500 });
    expect(h.some(s => s?.includes('matchString'))).toBe(true);
    expect(h.some(s => s?.includes('startLine'))).toBe(true);
  });
});
