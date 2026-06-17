import { describe, it, expect } from 'vitest';

import { hints as ripgrepHints } from '../../../../octocode-tools-core/src/tools/local_ripgrep/hints.js';
import { hints as findFilesHints } from '../../../../octocode-tools-core/src/tools/local_find_files/hints.js';
import { hints as viewStructureHints } from '../../../../octocode-tools-core/src/tools/local_view_structure/hints.js';
import { hints as fetchContentHints } from '../../../../octocode-tools-core/src/tools/local_fetch_content/hints.js';
import { hints as ghCodeHints } from '../../../../octocode-tools-core/src/tools/github_search_code/hints.js';
import { hints as ghFetchHints } from '../../../../octocode-tools-core/src/tools/github_fetch_content/hints.js';
import { hints as ghPrHints } from '../../../../octocode-tools-core/src/tools/github_search_pull_requests/hints.js';
import { hints as ghReposHints } from '../../../../octocode-tools-core/src/tools/github_search_repos/hints.js';
import { hints as ghViewHints } from '../../../../octocode-tools-core/src/tools/github_view_repo_structure/hints.js';
import { hints as cloneHints } from '../../../../octocode-tools-core/src/tools/github_clone_repo/hints.js';
import { hints as pkgHints } from '../../../../octocode-tools-core/src/tools/package_search/hints.js';
import { hints as semanticContentHints } from '../../../../octocode-tools-core/src/tools/lsp/semantic_content/hints.js';

import { buildPaginationHints } from '../../../../octocode-tools-core/src/tools/providerMappers.js';
import {
  generatePaginationHints,
  generateStructurePaginationHints,
} from '../../../../octocode-tools-core/src/utils/pagination/hints.js';

const ALL_HINTS = {
  localSearchCode: ripgrepHints,
  localFindFiles: findFilesHints,
  localViewStructure: viewStructureHints,
  localGetFileContent: fetchContentHints,
  ghSearchCode: ghCodeHints,
  ghGetFileContent: ghFetchHints,
  ghHistoryResearch: ghPrHints,
  ghSearchRepos: ghReposHints,
  ghViewRepoStructure: ghViewHints,
  ghCloneRepo: cloneHints,
  npmSearch: pkgHints,
  lspGetSemantics: semanticContentHints,
};

describe('per-tool hints — structural contract', () => {
  for (const [tool, gen] of Object.entries(ALL_HINTS)) {
    it(`${tool}: declares empty + error, never hasResults`, () => {
      expect(typeof gen.empty).toBe('function');
      expect(typeof gen.error).toBe('function');
      expect(
        (gen as unknown as Record<string, unknown>).hasResults
      ).toBeUndefined();
    });

    it(`${tool}: empty() with no context returns []`, () => {
      const out = gen
        .empty({})
        .filter((s): s is string => typeof s === 'string');
      expect(out).toEqual([]);
    });

    it(`${tool}: error() with no context returns []`, () => {
      const out = gen
        .error({})
        .filter((s): s is string => typeof s === 'string');
      expect(out).toEqual([]);
    });
  }
});

describe('localSearchCode (ripgrep) — empty permutations', () => {
  it('emits filter hint when langType is set', () => {
    const h = ripgrepHints.empty({ langType: 'ts', path: 'src' } as never);
    expect(h.length).toBeGreaterThan(0);
    expect(h.some(s => s?.includes('include/exclude/langType'))).toBe(true);
  });

  it('emits filter hint with include + excludeDir', () => {
    const h = ripgrepHints.empty({
      include: ['*.ts'],
      excludeDir: ['node_modules'],
    } as never);
    expect(h[0]).toContain('include/exclude/langType');
  });

  it('stays silent with no filters in play', () => {
    expect(ripgrepHints.empty({} as never)).toEqual([]);
  });
});

describe('localSearchCode (ripgrep) — error permutations', () => {
  it('size_limit with matchCount cites the count', () => {
    const h = ripgrepHints.error({
      errorType: 'size_limit',
      matchCount: 4200,
    } as never);
    expect(h[0]).toContain('4200');
  });

  it('size_limit without matchCount still emits a hint', () => {
    const h = ripgrepHints.error({ errorType: 'size_limit' } as never);
    expect(h[0]).toBeTruthy();
  });

  it('unknown errorType returns []', () => {
    expect(ripgrepHints.error({ errorType: 'timeout' as never })).toEqual([]);
  });
});

describe('localFindFiles — empty permutations', () => {
  it('returns a hint when name filter is set', () => {
    const h = findFilesHints.empty({ names: ['*.ts'], path: '/tmp' } as never);
    expect(h.length).toBeGreaterThan(0);
    expect(h[0]).toContain('filter');
  });

  it('returns a hint with multiple filters set', () => {
    const h = findFilesHints.empty({
      name: '*.md',
      modifiedWithin: '7d',
      sizeGreater: '10M',
    } as never);
    expect(h.length).toBeGreaterThan(0);
    expect(h[0]).toContain('filter');
  });

  it('stays silent without filters', () => {
    expect(findFilesHints.empty({ path: '/tmp' } as never)).toEqual([]);
  });
});

describe('localViewStructure — empty + error', () => {
  it('empty with extensions filter returns a hint', () => {
    const h = viewStructureHints.empty({
      path: 'src',
      extensions: ['ts'],
    } as never);
    expect(h.length).toBeGreaterThan(0);
    expect(h[0]).toContain('filter');
  });

  it('empty with pattern filter returns a hint', () => {
    const h = viewStructureHints.empty({
      pattern: '*.json',
    } as never);
    expect(h.length).toBeGreaterThan(0);
    expect(h[0]).toContain('filter');
  });

  it('empty stays silent without filters', () => {
    expect(viewStructureHints.empty({ path: 'src' } as never)).toEqual([]);
  });

  it('error size_limit returns [] (cap detection is a runtime warning, not a hint)', () => {
    const h = viewStructureHints.error({
      errorType: 'size_limit',
      entryCount: 12345,
    } as never);
    expect(h).toEqual([]);
  });
});

describe('localGetFileContent — empty + error', () => {
  it('empty returns []', () => {
    expect(fetchContentHints.empty({} as never)).toEqual([]);
  });

  it('error size_limit + isLarge emits KB size', () => {
    const h = fetchContentHints.error({
      errorType: 'size_limit',
      isLarge: true,
      fileSize: 500_000,
    } as never);
    expect(h[0]).toMatch(/~\d+KB/);
  });

  it('error size_limit without isLarge still emits hint with KB size', () => {
    const h = fetchContentHints.error({
      errorType: 'size_limit',
      fileSize: 1000,
    } as never);
    expect(h.length).toBeGreaterThan(0);
    expect(h[0]).toMatch(/too large|matchString/);
  });

  it('error size_limit with isLarge but no fileSize emits hint without KB', () => {
    const h = fetchContentHints.error({
      errorType: 'size_limit',
      isLarge: true,
    } as never);
    expect(h.length).toBeGreaterThan(0);
    expect(h[0]).toMatch(/too large|matchString/);
    expect(h[0]).not.toMatch(/~\d+KB/);
  });
});

describe('ghSearchCode — empty + error', () => {
  it('empty + owner/repo returns an actionable hint', () => {
    const h = ghCodeHints.empty({
      hasOwnerRepo: true,
      owner: 'facebook',
      repo: 'react',
    } as never);
    expect(h.length).toBeGreaterThan(0);
    expect(
      h.some(
        s =>
          s?.includes('unindexed') ||
          s?.includes('ghGetFileContent') ||
          s?.includes('ghViewRepoStructure') ||
          s?.includes('default branch')
      )
    ).toBe(true);
  });

  it('empty + filters returns filter removal hint', () => {
    const h = ghCodeHints.empty({
      hasOwnerRepo: true,
      owner: 'a',
      repo: 'b',
      extension: 'ts',
      path: 'src',
    } as never);
    expect(h.some(s => s?.includes('Remove path/filename/extension'))).toBe(
      true
    );
  });

  it('empty + single package-name keyword pivots to npmSearch', () => {
    const h = ghCodeHints.empty({
      hasOwnerRepo: false,
      keywords: ['@modelcontextprotocol/sdk'],
    } as never);
    expect(h.some(s => s?.includes('npmSearch'))).toBe(true);
  });

  it('empty + owner/repo includes ghGetFileContent fallback hint', () => {
    const h = ghCodeHints.empty({
      hasOwnerRepo: true,
      owner: 'facebookexperimental',
      repo: 'Recoil',
      keywords: ['useSyncExternalStore'],
    } as never);
    expect(h.some(s => s?.includes('ghGetFileContent'))).toBe(true);
  });

  it('empty + nonExistentScope: one concise scope hint, no archived noise', () => {
    const h = ghCodeHints.empty({
      hasOwnerRepo: true,
      owner: 'nope',
      repo: 'does-not-exist',
      nonExistentScope: true,
      keywords: ['foo'],
    } as never);
    expect(h).toHaveLength(1);
    expect(h[0]).toMatch(/exist|searchable/i);
    expect(h.some(s => s?.includes('archived'))).toBe(false);
    expect(h[0]!.length).toBeLessThan(120);
  });

  it('empty + path filter returns filter removal hint', () => {
    const h = ghCodeHints.empty({
      hasOwnerRepo: true,
      owner: 'a',
      repo: 'b',
      path: 'src',
      keywords: ['foo'],
    } as never);
    expect(h.some(s => s?.includes('Remove path/filename/extension'))).toBe(
      true
    );
    expect(h.some(s => s?.includes('single distinctive identifier'))).toBe(
      false
    );
  });

  it('empty + multi-word keyword gives broadening guidance', () => {
    const h = ghCodeHints.empty({
      hasOwnerRepo: true,
      owner: 'a',
      repo: 'b',
      keywords: ['export function parse'],
    } as never);
    expect(
      h.some(
        s =>
          s?.includes('unindexed') ||
          s?.includes('ghGetFileContent') ||
          s?.includes('ghViewRepoStructure') ||
          s?.includes('default branch')
      )
    ).toBe(true);
    expect(h.some(s => s?.includes('single distinctive identifier'))).toBe(
      false
    );
  });

  it('empty without context returns []', () => {
    expect(ghCodeHints.empty({} as never)).toEqual([]);
  });

  it('error rate-limit cites retry-after', () => {
    const h = ghCodeHints.error({
      isRateLimited: true,
      retryAfter: 60,
    } as never);
    expect(h[0]).toContain('Retry after 60s');
  });

  it('error 401', () => {
    const h = ghCodeHints.error({ status: 401 } as never);
    expect(h[0]).toContain('GITHUB_TOKEN');
  });

  it('error 403 (non-rate-limit) mentions scope', () => {
    const h = ghCodeHints.error({ status: 403 } as never);
    expect(h[0]).toContain('repo');
  });

  it('error 403 + rate-limit only emits rate-limit', () => {
    const h = ghCodeHints.error({
      status: 403,
      isRateLimited: true,
    } as never);
    expect(h).toHaveLength(1);
    expect(h[0]).toContain('Rate limited');
  });
});

describe('ghGetFileContent — error', () => {
  it('size_limit with KB', () => {
    const h = ghFetchHints.error({
      errorType: 'size_limit',
      fileSize: 400,
    } as never);
    expect(h[0]).toContain('400KB');
  });

  it('size_limit with totalLines pushes tail-read hint', () => {
    const h = ghFetchHints.error({
      errorType: 'size_limit',
      fileSize: 400,
      totalLines: 1000,
    } as never);
    expect(h.some(s => !!s && s.includes('1000'))).toBe(true);
  });

  it('not_found with branch emits omit-branch guidance', () => {
    const h = ghFetchHints.error({
      errorType: 'not_found',
      path: 'src/index.ts',
      branch: 'main',
    } as never);
    expect(h.some(s => s?.includes('branch'))).toBe(true);
    expect(h[0]).toContain('ghViewRepoStructure');
  });

  it('not_found without branch is still actionable', () => {
    const h = ghFetchHints.error({
      errorType: 'not_found',
      path: 'README.md',
    } as never);
    expect(h[0]).toContain('ghViewRepoStructure');
  });

  it('rate-limited with retryAfter', () => {
    const h = ghFetchHints.error({
      isRateLimited: true,
      retryAfter: 30,
    } as never);
    expect(h[0]).toContain('Retry after 30s');
  });

  it('rate-limited without retryAfter says wait', () => {
    const h = ghFetchHints.error({ isRateLimited: true } as never);
    expect(h[0]).toContain('Wait before retrying');
  });

  it('status 401 returns token error', () => {
    const h = ghFetchHints.error({ status: 401 } as never);
    expect(h[0]).toContain('GITHUB_TOKEN');
  });

  it('status 403 returns scope error', () => {
    const h = ghFetchHints.error({ status: 403 } as never);
    expect(h[0]).toContain('repo');
  });
});

describe('ghHistoryResearch — empty permutations', () => {
  it('prNumber not found gives recovery hint', () => {
    const h = ghPrHints.empty({
      prNumber: 999,
      owner: 'a',
      repo: 'b',
    } as never);
    expect(h[0]).toContain('PR number');
  });

  it('merged state shows widening hint for empty results', () => {
    const h = ghPrHints.empty({
      state: 'merged',
      author: 'alice',
      query: 'fix bug',
      owner: 'a',
      repo: 'b',
    } as never);
    expect(h[0]).toContain('merged');
  });

  it('stays silent without filters', () => {
    expect(ghPrHints.empty({} as never)).toEqual([]);
  });

  it('returns [] when scope is defined but no state/author/query filters', () => {
    const h = ghPrHints.empty({ owner: 'a', repo: 'b' } as never);
    expect(h).toEqual([]);
  });

  it('non-merged state shows loose filter hint not merged-specific', () => {
    const h = ghPrHints.empty({
      state: 'open',
      owner: 'a',
      repo: 'b',
    } as never);
    expect(h[0]).toContain('filter');
    expect(h[0]).not.toContain('is:merged');
  });

  it('emits the generic filter-removal hint for non-merged states', () => {
    const withAuthor = ghPrHints.empty({
      state: 'open',
      author: 'alice',
      owner: 'a',
      repo: 'b',
    } as never);
    expect(withAuthor[0]).toContain('filter');

    const withQuery = ghPrHints.empty({
      state: 'open',
      query: 'fix bug',
      owner: 'a',
      repo: 'b',
    } as never);
    expect(withQuery[0]).toContain('filter');
    expect(withQuery[0]).not.toContain('is:merged');
  });

  it('no query shows add-query hint', () => {
    const h = ghPrHints.empty({
      state: 'open',
      owner: 'a',
      repo: 'b',
    } as never);
    expect(h[1]).toContain('keyword');
  });
});

describe('ghHistoryResearch — error permutations', () => {
  it('rate-limited with retryAfter includes retry time', () => {
    const h = ghPrHints.error({
      isRateLimited: true,
      retryAfter: 45,
    } as never);
    expect(h[0]).toContain('Retry after 45s');
  });

  it('rate-limited without retryAfter says wait', () => {
    const h = ghPrHints.error({
      isRateLimited: true,
    } as never);
    expect(h[0]).toContain('Wait before retrying');
  });

  it('status 401 returns token error', () => {
    const h = ghPrHints.error({ status: 401 } as never);
    expect(h[0]).toContain('GITHUB_TOKEN');
  });

  it('status 403 returns scope error', () => {
    const h = ghPrHints.error({ status: 403 } as never);
    expect(h[0]).toContain('repo');
  });

  it('unknown error returns []', () => {
    expect(ghPrHints.error({ status: 500 } as never)).toEqual([]);
  });

  it('empty with scoped repo and query returns filter removal hint', () => {
    const h = ghPrHints.empty({
      owner: 'acme',
      repo: 'utils',
      query: 'fix bug',
    } as never);
    expect(h.some(s => /remove|broader/i.test(s ?? ''))).toBe(true);
  });
});

describe('ghSearchRepos — hints coverage', () => {
  it('empty returns [] when no query and no filters', () => {
    expect(ghReposHints.empty({} as never)).toEqual([]);
  });

  it('empty includes query-driven guidance', () => {
    const h = ghReposHints.empty({ query: 'react' } as never);
    expect(h.length).toBeGreaterThan(0);
    expect(h[0]).toContain('fewer/simpler keywords');
  });

  it('empty includes filter-widening guidance', () => {
    const h = ghReposHints.empty({
      keywordsToSearch: ['router'],
      language: 'TypeScript',
      owner: 'wix-private',
    } as never);
    expect(h[0]).toContain('Remove owner/language/topic');
  });

  it('empty suggests npmSearch for package-like terms', () => {
    const h = ghReposHints.empty({ query: '@babel/core' } as never);
    expect(h.some(s => (s ?? '').includes('use `npmSearch`'))).toBe(true);
  });

  it('empty does NOT suggest npmSearch for camelCase identifiers', () => {
    const camelCases = [
      'lspGetSemantics',
      'withSecurityValidation',
      'executeCloneRepo',
      'MyComponent',
    ];
    for (const term of camelCases) {
      const h = ghReposHints.empty({ query: term } as never);
      expect(h.some(s => (s ?? '').includes('npmSearch'))).toBe(
        false,
        `"${term}" should NOT trigger npmSearch hint`
      );
    }
  });

  it('empty DOES suggest npmSearch for kebab/dot/scoped package names', () => {
    const packageLike = ['react-query', 'lodash.get', '@scope/pkg'];
    for (const term of packageLike) {
      const h = ghReposHints.empty({ query: term } as never);
      expect(h.some(s => (s ?? '').includes('npmSearch'))).toBe(
        true,
        `"${term}" should trigger npmSearch hint`
      );
    }
  });

  it('error rate-limited with retryAfter', () => {
    const h = ghReposHints.error({
      isRateLimited: true,
      retryAfter: 42,
    } as never);
    expect(h[0]).toContain('Retry after 42s');
  });

  it('error 401 token issue', () => {
    const h = ghReposHints.error({ status: 401 } as never);
    expect(h[0]).toContain('missing or expired');
  });

  it('error 403 scope issue', () => {
    const h = ghReposHints.error({ status: 403 } as never);
    expect(h[0]).toContain('public_repo');
  });

  it('error returns [] for unknown status', () => {
    expect(ghReposHints.error({} as never)).toEqual([]);
  });
});

describe('ghViewRepoStructure — empty', () => {
  it('cites path when set', () => {
    const h = ghViewHints.empty({ path: 'src', branch: 'dev' } as never);
    expect(h.length).toBeGreaterThan(0);
    expect(h[0]).toContain('parent');
  });

  it('returns [] when only branch is provided (no actionable hint)', () => {
    const h = ghViewHints.empty({ branch: 'dev' } as never);
    expect(h).toEqual([]);
  });

  it('stays silent when both path and branch missing', () => {
    expect(ghViewHints.empty({} as never)).toEqual([]);
  });
});

describe('ghCloneRepo — error', () => {
  it('permission', () => {
    expect(cloneHints.error({ errorType: 'permission' } as never)[0]).toContain(
      'Token'
    );
  });

  it('not_found', () => {
    expect(cloneHints.error({ errorType: 'not_found' } as never)[0]).toContain(
      'branch'
    );
  });

  it('timeout', () => {
    expect(cloneHints.error({ errorType: 'timeout' } as never)[0]).toContain(
      'timed out'
    );
  });

  it('unknown returns []', () => {
    expect(cloneHints.error({ errorType: 'other' as never })).toEqual([]);
  });

  it('rate-limited with retryAfter', () => {
    const h = cloneHints.error({
      isRateLimited: true,
      retryAfter: 20,
    } as never);
    expect(h[0]).toContain('Retry after 20s');
  });

  it('rate-limited without retryAfter says wait', () => {
    const h = cloneHints.error({ isRateLimited: true } as never);
    expect(h[0]).toContain('Wait before retrying');
  });
});

describe('ghCloneRepo — empty', () => {
  it('returns [] when no sparsePath', () => {
    expect(cloneHints.empty({} as never)).toEqual([]);
  });

  it('returns guidance when sparsePath provided but matched nothing', () => {
    const h = cloneHints.empty({ sparsePath: 'src/utils' } as never);
    expect(h[0]).toContain('sparsePath');
    expect(h[1]).toContain('ghViewRepoStructure');
  });
});

describe('npmSearch — hints coverage', () => {
  it('empty returns [] when no name context', () => {
    expect(pkgHints.empty({} as never)).toEqual([]);
  });

  it('empty returns package guidance when name exists', () => {
    const h = pkgHints.empty({ name: 'left-pad' } as never);
    expect(h.length).toBeGreaterThan(0);
    expect(h[0]).toContain('version suffix');
  });

  it('empty includes npm alternative hint', () => {
    const h = pkgHints.empty({ name: 'lodash' } as never);
    expect(h.some(s => s?.includes('ghSearchRepos'))).toBe(true);
  });

  it('error rate-limited includes retryAfter when present', () => {
    const h = pkgHints.error({
      isRateLimited: true,
      retryAfter: 11,
    } as never);
    expect(h[0]).toContain('Retry after 11s');
  });

  it('error rate-limited without retryAfter still guides waiting', () => {
    const h = pkgHints.error({ isRateLimited: true } as never);
    expect(h[0]).toContain('Wait before retrying');
  });

  it('error returns [] when no known error context', () => {
    expect(pkgHints.error({} as never)).toEqual([]);
  });
});

describe('lspGetSemantics — empty + error', () => {
  it('empty with symbolName returns a recovery hint', () => {
    const h = semanticContentHints.empty({
      symbolName: 'handleAuth',
    } as never);
    expect(h.length).toBeGreaterThan(0);
    expect(h[0]).toContain('localSearchCode');
  });

  it('error symbol_not_found tells agents to refresh lineHint', () => {
    const h = semanticContentHints.error({
      errorType: 'symbol_not_found',
    } as never);
    expect(h[0]).toContain('lineHint');
  });

  it('error lsp_unavailable gives local fallback guidance', () => {
    const h = semanticContentHints.error({
      errorType: 'lsp_unavailable',
    } as never);
    expect(h[0]).toContain('localSearchCode');
  });
});

describe('pagination hints — fire only on hasMore=true', () => {
  describe('buildPaginationHints (GitHub search-style)', () => {
    it('emits a Next: page=N+1 cursor when hasMore', () => {
      const h = buildPaginationHints(
        {
          currentPage: 1,
          totalPages: 3,
          hasMore: true,
          perPage: 10,
          totalMatches: 25,
        },
        'matches'
      );
      expect(h[0]).toContain('Page 1/3');
      expect(h[0]).toContain('Next: page=2');
    });

    it('stays silent when totalPages <= 1', () => {
      const h = buildPaginationHints(
        {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          perPage: 10,
          totalMatches: 5,
        },
        'matches'
      );
      expect(h).toEqual([]);
    });

    it('stays silent on the final page (no tautology)', () => {
      const h = buildPaginationHints(
        {
          currentPage: 3,
          totalPages: 3,
          hasMore: false,
          perPage: 10,
          totalMatches: 25,
        },
        'matches'
      );
      expect(h).toEqual([]);
    });
  });

  describe('generatePaginationHints (local tools, char-offset based)', () => {
    const basePaginationMetadata = {
      paginatedContent: '',
      byteOffset: 0,
      byteLength: 0,
      totalBytes: 0,
      charOffset: 0,
      charLength: 0,
      totalChars: 0,
    };

    it('emits charOffset cursor when hasMore + nextCharOffset', () => {
      const h = generatePaginationHints({
        ...basePaginationMetadata,
        currentPage: 1,
        totalPages: 4,
        hasMore: true,
        nextCharOffset: 1000,
      });
      expect(h.some(s => s.includes('charOffset=1000'))).toBe(true);
    });

    it('stays silent when hasMore=false', () => {
      const h = generatePaginationHints({
        ...basePaginationMetadata,
        currentPage: 4,
        totalPages: 4,
        hasMore: false,
      });
      expect(h).toEqual([]);
    });

    it('emits token warning when estimatedTokens > 30k', () => {
      const h = generatePaginationHints({
        ...basePaginationMetadata,
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        estimatedTokens: 35_000,
      });
      expect(h.some(s => s.includes('30k') || s.includes('35,000'))).toBe(true);
    });

    it('suppresses token warning under 30k', () => {
      const h = generatePaginationHints({
        ...basePaginationMetadata,
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        estimatedTokens: 5_000,
      });
      expect(h.some(s => s.includes('tokens'))).toBe(false);
    });
  });

  describe('current LSP unavailable error types', () => {
    it('semantic-content unavailable emits localSearchCode guidance', () => {
      const h = semanticContentHints
        .error({ errorType: 'lsp_unavailable' as never })
        .filter((s): s is string => typeof s === 'string');
      expect(h.some(s => s.includes('localSearchCode'))).toBe(true);
    });
  });

  describe('generateStructurePaginationHints (repo structure)', () => {
    it('emits page cursor', () => {
      const h = generateStructurePaginationHints(
        {
          currentPage: 1,
          totalPages: 3,
          hasMore: true,
          entriesPerPage: 10,
          totalEntries: 30,
        },
        {} as never
      );
      expect(h[0]).toContain('page=2');
    });

    it('stays silent without hasMore', () => {
      const h = generateStructurePaginationHints(
        {
          currentPage: 3,
          totalPages: 3,
          hasMore: false,
          entriesPerPage: 10,
          totalEntries: 30,
        },
        {} as never
      );
      expect(h).toEqual([]);
    });
  });
});
