import { describe, it, expect } from 'vitest';

import { hints as ripgrepHints } from '../../../src/tools/local_ripgrep/hints.js';
import { hints as findFilesHints } from '../../../src/tools/local_find_files/hints.js';
import { hints as viewStructureHints } from '../../../src/tools/local_view_structure/hints.js';
import { hints as fetchContentHints } from '../../../src/tools/local_fetch_content/hints.js';
import { hints as ghCodeHints } from '../../../src/tools/github_search_code/hints.js';
import { hints as ghFetchHints } from '../../../src/tools/github_fetch_content/hints.js';
import { hints as ghPrHints } from '../../../src/tools/github_search_pull_requests/hints.js';
import { hints as ghReposHints } from '../../../src/tools/github_search_repos/hints.js';
import { hints as ghViewHints } from '../../../src/tools/github_view_repo_structure/hints.js';
import { hints as cloneHints } from '../../../src/tools/github_clone_repo/hints.js';
import { hints as pkgHints } from '../../../src/tools/package_search/hints.js';
import { hints as gotoHints } from '../../../src/tools/lsp_goto_definition/hints.js';
import { hints as refsHints } from '../../../src/tools/lsp_find_references/hints.js';
import { hints as callHints } from '../../../src/tools/lsp_call_hierarchy/hints.js';

import { buildPaginationHints } from '../../../src/tools/providerMappers.js';
import {
  generatePaginationHints,
  generateGitHubPaginationHints,
  generateStructurePaginationHints,
} from '../../../src/utils/pagination/hints.js';

const ALL_HINTS = {
  localSearchCode: ripgrepHints,
  localFindFiles: findFilesHints,
  localViewStructure: viewStructureHints,
  localGetFileContent: fetchContentHints,
  githubSearchCode: ghCodeHints,
  githubGetFileContent: ghFetchHints,
  githubSearchPullRequests: ghPrHints,
  githubSearchRepositories: ghReposHints,
  githubViewRepoStructure: ghViewHints,
  githubCloneRepo: cloneHints,
  packageSearch: pkgHints,
  lspGotoDefinition: gotoHints,
  lspFindReferences: refsHints,
  lspCallHierarchy: callHints,
};

describe('per-tool hints — structural contract', () => {
  for (const [tool, gen] of Object.entries(ALL_HINTS)) {
    it(`${tool}: declares empty + error, never hasResults`, () => {
      expect(typeof gen.empty).toBe('function');
      expect(typeof gen.error).toBe('function');
      expect((gen as Record<string, unknown>).hasResults).toBeUndefined();
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
  it('emits filter list when type is set', () => {
    const h = ripgrepHints.empty({ type: 'ts', path: 'src' } as never);
    expect(h.some(s => s?.includes('type="ts"'))).toBe(true);
    expect(h.some(s => s?.includes("'src'") || s?.includes('src'))).toBe(true);
  });

  it('emits filter list with include + excludeDir', () => {
    const h = ripgrepHints.empty({
      include: ['*.ts'],
      excludeDir: ['node_modules'],
    } as never);
    expect(h[0]).toContain('include=');
    expect(h[0]).toContain('excludeDir=');
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
  it('quotes name filter', () => {
    const h = findFilesHints.empty({ name: '*.ts', path: '/tmp' } as never);
    expect(h[0]).toContain('name="*.ts"');
    expect(h[0]).toContain('/tmp');
  });

  it('joins multiple filters with +', () => {
    const h = findFilesHints.empty({
      name: '*.md',
      modifiedWithin: '7d',
      sizeGreater: '10M',
    } as never);
    expect(h[0]).toMatch(/name=.+\+.+modifiedWithin=.+\+.+sizeGreater=/);
  });

  it('stays silent without filters', () => {
    expect(findFilesHints.empty({ path: '/tmp' } as never)).toEqual([]);
  });
});

describe('localViewStructure — empty + error', () => {
  it('empty with extension filter', () => {
    const h = viewStructureHints.empty({
      path: 'src',
      extension: 'ts',
    } as never);
    expect(h[0]).toContain('extension="ts"');
  });

  it('empty with pattern filter', () => {
    const h = viewStructureHints.empty({
      pattern: '*.json',
    } as never);
    expect(h[0]).toContain('pattern="*.json"');
  });

  it('empty stays silent without filters', () => {
    expect(viewStructureHints.empty({ path: 'src' } as never)).toEqual([]);
  });

  it('error size_limit emits entry count', () => {
    const h = viewStructureHints.error({
      errorType: 'size_limit',
      entryCount: 12345,
    } as never);
    expect(h[0]).toContain('12345');
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
    expect(h[0]).toMatch(/read budget/);
  });

  it('error size_limit with isLarge but no fileSize emits hint without KB', () => {
    const h = fetchContentHints.error({
      errorType: 'size_limit',
      isLarge: true,
    } as never);
    expect(h.length).toBeGreaterThan(0);
    expect(h[0]).toMatch(/read budget/);
    expect(h[0]).not.toMatch(/KB/);
  });
});

describe('githubSearchCode — empty + error', () => {
  it('empty + owner/repo names the scope', () => {
    const h = ghCodeHints.empty({
      hasOwnerRepo: true,
      owner: 'facebook',
      repo: 'react',
    } as never);
    expect(h[0]).toContain('facebook/react');
  });

  it('empty + filters cites them inline', () => {
    const h = ghCodeHints.empty({
      hasOwnerRepo: true,
      owner: 'a',
      repo: 'b',
      extension: 'ts',
      path: 'src',
    } as never);
    expect(h[0]).toContain('extension+path');
  });

  it('empty + single package-name keyword pivots to packageSearch', () => {
    const h = ghCodeHints.empty({
      hasOwnerRepo: false,
      keywords: ['@modelcontextprotocol/sdk'],
    } as never);
    expect(h.some(s => s?.includes('packageSearch'))).toBe(true);
  });

  it('empty + owner/repo always warns archived repos may be unindexed (SC-4)', () => {
    const h = ghCodeHints.empty({
      hasOwnerRepo: true,
      owner: 'facebookexperimental',
      repo: 'Recoil',
      keywords: ['useSyncExternalStore'],
    } as never);
    expect(h.some(s => s?.includes('archived'))).toBe(true);
    expect(
      h.some(
        s => s?.includes('githubGetFileContent') && s?.includes('"not found"')
      )
    ).toBe(true);
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

  it('empty + path filter explains path: is directory-only (SC-2 recovery)', () => {
    const h = ghCodeHints.empty({
      hasOwnerRepo: true,
      owner: 'a',
      repo: 'b',
      path: 'src',
      keywords: ['foo'],
    } as never);
    expect(h.some(s => s?.includes('directory'))).toBe(true);
    expect(h.some(s => s?.includes('filename:'))).toBe(true);
    expect(h.some(s => s?.includes('single distinctive identifier'))).toBe(
      false
    );
  });

  it('empty + multi-word keyword (no path) gives phrase-broadening guidance', () => {
    const h = ghCodeHints.empty({
      hasOwnerRepo: true,
      owner: 'a',
      repo: 'b',
      keywords: ['export function parse'],
    } as never);
    expect(h.some(s => s?.includes('phrase'))).toBe(true);
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

describe('githubGetFileContent — error', () => {
  it('size_limit with KB', () => {
    const h = ghFetchHints.error({
      errorType: 'size_limit',
      fileSize: 400,
    } as never);
    expect(h[0]).toContain('400KB');
  });

  it('not_found with path + branch', () => {
    const h = ghFetchHints.error({
      errorType: 'not_found',
      path: 'src/index.ts',
      branch: 'main',
    } as never);
    expect(h[0]).toContain('src/index.ts');
    expect(h[0]).toContain('main');
  });

  it('not_found without branch is still actionable', () => {
    const h = ghFetchHints.error({
      errorType: 'not_found',
      path: 'README.md',
    } as never);
    expect(h[0]).toContain('README.md');
  });
});

describe('githubSearchPullRequests — empty permutations', () => {
  it('prNumber not found', () => {
    const h = ghPrHints.empty({
      prNumber: 999,
      owner: 'a',
      repo: 'b',
    } as never);
    expect(h[0]).toContain('PR #999');
    expect(h[0]).toContain('a/b');
  });

  it('lists state + author + query filters', () => {
    const h = ghPrHints.empty({
      state: 'merged',
      author: 'alice',
      query: 'fix bug',
      owner: 'a',
      repo: 'b',
    } as never);
    expect(h[0]).toContain('state=merged');
    expect(h[0]).toContain('author=alice');
    expect(h[0]).toContain('query="fix bug"');
  });

  it('stays silent without filters', () => {
    expect(ghPrHints.empty({} as never)).toEqual([]);
  });
});

describe('githubSearchRepositories — hints coverage', () => {
  it('empty returns [] when no query and no filters', () => {
    expect(ghReposHints.empty({} as never)).toEqual([]);
  });

  it('empty includes query-driven guidance', () => {
    const h = ghReposHints.empty({ query: 'react' } as never);
    expect(h[0]).toContain('No repositories found for "react"');
    expect(h[1]).toContain('fewer/simpler keywords');
  });

  it('empty includes filter-widening guidance', () => {
    const h = ghReposHints.empty({
      keywordsToSearch: ['router'],
      language: 'TypeScript',
      owner: 'wix-private',
    } as never);
    expect(h[0]).toContain('router');
    expect(h[1]).toContain('Remove filters one at a time');
  });

  it('empty suggests packageSearch for package-like terms', () => {
    const h = ghReposHints.empty({ query: '@babel/core' } as never);
    expect(h.some(s => (s ?? '').includes('use `packageSearch`'))).toBe(true);
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

describe('githubViewRepoStructure — empty', () => {
  it('cites path + branch when both set', () => {
    const h = ghViewHints.empty({ path: 'src', branch: 'dev' } as never);
    expect(h[0]).toContain("'src'");
    expect(h[0]).toContain("'dev'");
  });

  it('cites branch only when path missing', () => {
    const h = ghViewHints.empty({ branch: 'dev' } as never);
    expect(h[0]).toContain('root');
    expect(h[0]).toContain("'dev'");
  });

  it('stays silent when both path and branch missing', () => {
    expect(ghViewHints.empty({} as never)).toEqual([]);
  });
});

describe('githubCloneRepo — error', () => {
  it('permission', () => {
    expect(cloneHints.error({ errorType: 'permission' } as never)[0]).toContain(
      'Token'
    );
  });

  it('not_found', () => {
    expect(cloneHints.error({ errorType: 'not_found' } as never)[0]).toContain(
      'not found'
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
});

describe('packageSearch — hints coverage', () => {
  it('empty returns [] when no name context', () => {
    expect(pkgHints.empty({} as never)).toEqual([]);
  });

  it('empty returns package guidance when query exists', () => {
    const h = pkgHints.empty({ query: 'left-pad' } as never);
    expect(h[0]).toContain("Package 'left-pad' not found on npm.");
    expect(h[1]).toContain('remove any version suffix');
  });

  it('empty resolves name from keywords array', () => {
    const h = pkgHints.empty({ keywords: ['lodash'] } as never);
    expect(h[0]).toContain("Package 'lodash' not found on npm.");
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

describe('lspGotoDefinition — empty + error', () => {
  it('empty with searchRadius + lineHint', () => {
    const h = gotoHints.empty({
      searchRadius: 2,
      lineHint: 42,
    } as never);
    expect(h[0]).toContain('±2');
    expect(h[0]).toContain('42');
  });

  it('empty without searchRadius is silent', () => {
    expect(gotoHints.empty({ lineHint: 5 } as never)).toEqual([]);
  });

  it('error symbol_not_found cites symbol + line', () => {
    const h = gotoHints.error({
      errorType: 'symbol_not_found',
      symbolName: 'handleAuth',
      lineHint: 30,
    } as never);
    expect(h[0]).toContain('handleAuth');
    expect(h[0]).toContain('30');
  });

  it('error file_not_found cites uri', () => {
    const h = gotoHints.error({
      errorType: 'file_not_found',
      uri: 'src/missing.ts',
    } as never);
    expect(h[0]).toContain('src/missing.ts');
  });

  it('error timeout', () => {
    const h = gotoHints.error({ errorType: 'timeout' } as never);
    expect(h[0]).toContain('timed out');
  });
});

describe('lspFindReferences — empty', () => {
  it('filteredAll → broaden include/exclude', () => {
    const h = refsHints.empty({ filteredAll: true } as never);
    expect(h[0]).toContain('include/exclude');
  });

  it('silent otherwise', () => {
    expect(refsHints.empty({} as never)).toEqual([]);
  });
});

describe('lspCallHierarchy — error', () => {
  it('not_a_function', () => {
    const h = callHints.error({
      errorType: 'not_a_function',
    } as never);
    expect(h[0]).toContain('not a function');
  });

  it('timeout cites depth', () => {
    const h = callHints.error({
      errorType: 'timeout',
      depth: 5,
    } as never);
    expect(h[0]).toContain('Depth=5');
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
    it('emits charOffset cursor when hasMore + nextCharOffset', () => {
      const h = generatePaginationHints({
        currentPage: 1,
        totalPages: 4,
        hasMore: true,
        nextCharOffset: 1000,
      });
      expect(h.some(s => s.includes('charOffset=1000'))).toBe(true);
    });

    it('stays silent when hasMore=false', () => {
      const h = generatePaginationHints({
        currentPage: 4,
        totalPages: 4,
        hasMore: false,
      });
      expect(h).toEqual([]);
    });

    it('emits token warning when estimatedTokens > 30k', () => {
      const h = generatePaginationHints({
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        estimatedTokens: 35_000,
      });
      expect(h.some(s => s.includes('30k') || s.includes('35,000'))).toBe(true);
    });

    it('suppresses token warning under 30k', () => {
      const h = generatePaginationHints({
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        estimatedTokens: 5_000,
      });
      expect(h.some(s => s.includes('tokens'))).toBe(false);
    });
  });

  describe('generateGitHubPaginationHints (file content)', () => {
    it('emits charOffset cursor when hasMore', () => {
      const h = generateGitHubPaginationHints(
        {
          currentPage: 1,
          totalPages: 5,
          hasMore: true,
          byteOffset: 1000,
          byteLength: 500,
        },
        {} as never
      );
      expect(h[0]).toContain('charOffset=1500');
    });

    it('stays silent on final page', () => {
      const h = generateGitHubPaginationHints(
        {
          currentPage: 5,
          totalPages: 5,
          hasMore: false,
        },
        {} as never
      );
      expect(h).toEqual([]);
    });
  });

  describe('lspCallHierarchy — lsp_unavailable error type', () => {
    it('error with lsp_unavailable emits localSearchCode guidance', () => {
      const h = callHints
        .error({ errorType: 'lsp_unavailable' as never, symbolName: 'myFn' })
        .filter((s): s is string => typeof s === 'string');
      expect(h.some(s => s.includes('localSearchCode'))).toBe(true);
    });

    it('error with lsp_unavailable and symbolName names the symbol', () => {
      const h = callHints
        .error({ errorType: 'lsp_unavailable' as never, symbolName: 'myFn' })
        .filter((s): s is string => typeof s === 'string');
      expect(h.some(s => s.includes('myFn'))).toBe(true);
    });

    it('error with no context still returns []', () => {
      const h = callHints
        .error({})
        .filter((s): s is string => typeof s === 'string');
      expect(h).toEqual([]);
    });
  });

  describe('lspFindReferences — lsp_unavailable error type', () => {
    it('error with lsp_unavailable emits localSearchCode guidance', () => {
      const h = refsHints
        .error({ errorType: 'lsp_unavailable' as never, symbolName: 'myFn' })
        .filter((s): s is string => typeof s === 'string');
      expect(h.some(s => s.includes('localSearchCode'))).toBe(true);
    });

    it('error with lsp_unavailable and symbolName names the symbol', () => {
      const h = refsHints
        .error({ errorType: 'lsp_unavailable' as never, symbolName: 'myFn' })
        .filter((s): s is string => typeof s === 'string');
      expect(h.some(s => s.includes('myFn'))).toBe(true);
    });
  });

  describe('generateStructurePaginationHints (repo structure)', () => {
    it('emits entryPageNumber cursor', () => {
      const h = generateStructurePaginationHints(
        {
          currentPage: 1,
          totalPages: 3,
          hasMore: true,
        },
        {} as never
      );
      expect(h[0]).toContain('entryPageNumber=2');
    });

    it('stays silent without hasMore', () => {
      const h = generateStructurePaginationHints(
        {
          currentPage: 3,
          totalPages: 3,
          hasMore: false,
        },
        {} as never
      );
      expect(h).toEqual([]);
    });
  });
});
