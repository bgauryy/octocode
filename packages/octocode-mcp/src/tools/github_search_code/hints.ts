import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

const TEST_INFRA_KEYWORDS = new Set([
  'jest',
  'vitest',
  'mocha',
  'jasmine',
  'karma',
  'playwright',
  'cypress',
  'testing-library',
  'test-runner',
  'test-utils',
  '__tests__',
  '__mocks__',
  'setupTests',
  'setupFilesAfterFramework',
  'testEnvironment',
  'testMatch',
  'coverageThreshold',
  'spec',
  'e2e',
  'fixture',
  'mock',
  'stub',
  'spy',
]);

function looksLikeTestInfrastructureQuery(
  keywords: unknown[] | undefined
): boolean {
  if (!keywords) return false;
  return keywords.some(
    k =>
      typeof k === 'string' &&
      TEST_INFRA_KEYWORDS.has(k.toLowerCase().replace(/[-_.]/g, ''))
  );
}

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const out: string[] = [];
    const c = ctx as Record<string, unknown>;
    const keywords = Array.isArray(c.keywords) ? c.keywords : undefined;
    const owner = typeof c.owner === 'string' ? c.owner : undefined;
    const repo = typeof c.repo === 'string' ? c.repo : undefined;
    const filters: string[] = [];
    if (typeof c.extension === 'string') filters.push('extension');
    if (typeof c.filename === 'string') filters.push('filename');
    if (typeof c.path === 'string') filters.push('path');

    if (c.nonExistentScope === true) {
      const scope = owner && repo ? `${owner}/${repo}` : owner || 'target';
      out.push(
        `"${scope}" doesn't exist or isn't searchable (not "no matches") — check spelling/access.`
      );
      return out;
    }

    if (ctx.hasOwnerRepo && owner && repo) {
      const filterList = filters.length > 0 ? ` (${filters.join('+')})` : '';
      out.push(`No matches in ${owner}/${repo}${filterList}.`);

      const rawPath = typeof c.path === 'string' ? c.path : undefined;
      const pathLooksLikeFile =
        rawPath &&
        !c.filename &&
        /(?:^|\/)([^/]+\.[A-Za-z][A-Za-z0-9]{0,9})$/.test(rawPath);
      if (pathLooksLikeFile) {
        const extracted = rawPath.match(
          /(?:^|\/)([^/]+\.[A-Za-z][A-Za-z0-9]{0,9})$/
        );
        const fname = extracted ? extracted[1] : rawPath;
        const dir = extracted
          ? rawPath.slice(0, extracted.index) || undefined
          : undefined;
        out.push(
          `path="${rawPath}" looks like a file path — GitHub auto-extracted filename="${fname}"${dir ? ` + path="${dir}"` : ''} for the query, but the file was not found. Try: (1) use explicit filename="${fname}" without path; (2) broaden to path="${dir ?? rawPath.split('/').slice(0, -1).join('/')}".`
        );
      }

      const hasPhrase =
        Array.isArray(keywords) &&
        keywords.some(k => typeof k === 'string' && /\s/.test(k));
      if (filters.includes('extension') || filters.includes('filename')) {
        out.push(
          'extension: and filename: filters stack with AND and silently zero out results — remove them and search with keywords only, then re-add once you have hits.'
        );
      } else if (filters.includes('path') && !pathLooksLikeFile) {
        out.push(
          'GitHub path: matches a directory, not a file — broaden path: to a parent directory (use filename: to target one file).'
        );
      } else if (hasPhrase) {
        out.push(
          'A multi-word phrase is matched literally — broaden with fewer/looser keyword terms.'
        );
      }
      out.push(
        'A zero here isn\'t proof — code search misses archived repos and is stale for renamed/redirected ones; confirm via githubGetFileContent (it follows redirects) before "not found".'
      );
    }

    if (
      !ctx.hasOwnerRepo &&
      keywords &&
      keywords.length === 1 &&
      typeof keywords[0] === 'string' &&
      // Match scoped packages (@scope/pkg) or kebab/dot-separated names (react-query, lodash.get)
      // Excludes camelCase/PascalCase identifiers like lspGetSemanticContent or withSecurityValidation
      /^@[\w-]+\/[\w.-]+$|^[a-z][\w]*[-.][\w.-]+$/.test(keywords[0])
    ) {
      out.push(
        `"${keywords[0]}" looks like a package name — try packageSearch.`
      );
    }

    if (
      !ctx.hasOwnerRepo &&
      out.length === 0 &&
      keywords &&
      keywords.length > 0
    ) {
      out.push(
        'No matches across GitHub — scope to owner/repo, run separate single-term queries, or add extension/path filters.'
      );
      if (filters.includes('path')) {
        out.push(
          'GitHub path: matches a directory prefix, not a full path — broaden or omit path to search the whole repo.'
        );
      }
    }

    // Test infrastructure queries often fail because agents filter to src/.
    // Surface direct test-directory searches when keywords imply testing topics.
    if (looksLikeTestInfrastructureQuery(keywords)) {
      out.push(
        'Testing infrastructure query detected — search directly in test directories: use path=__tests__, filename=jest.config, filename=vitest.config, or filename=jest.setup. Do NOT restrict to path=src when looking for test runner setup.'
      );
    }

    return out;
  },

  error: (ctx: HintContext = {}) => {
    const out: string[] = [];
    if (ctx.isRateLimited) {
      out.push(
        `Rate limited.${ctx.retryAfter ? ` Retry after ${ctx.retryAfter}s.` : ''}`
      );
    }
    if (ctx.status === 401) {
      out.push('GITHUB_TOKEN missing/expired.');
    }
    if (ctx.status === 403 && !ctx.isRateLimited) {
      out.push('Token lacks `repo` scope.');
    }
    return out;
  },
};
