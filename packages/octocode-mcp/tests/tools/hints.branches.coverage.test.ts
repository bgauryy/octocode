import { describe, it, expect } from 'vitest';

describe('github_view_repo_structure hints — uncovered branches', () => {
  let hints: (typeof import('../../../octocode-tools-core/src/tools/github_view_repo_structure/hints.js'))['hints'];

  beforeAll(async () => {
    ({ hints } =
      await import('../../../octocode-tools-core/src/tools/github_view_repo_structure/hints.js'));
  });

  it('empty() returns [] when no path and no branch', () => {
    expect(hints.empty({})).toEqual([]);
  });

  it('empty() returns hints when only path is provided', () => {
    const result = hints.empty({ path: 'src' } as never);
    expect(result.length).toBeGreaterThan(0);
    expect(result[0]).toContain('parent');
  });

  it('empty() returns [] when only branch is provided (no actionable hint without path)', () => {
    const result = hints.empty({ branch: 'main' } as never);
    expect(result).toEqual([]);
  });

  it('error() rate-limited — with retryAfter', () => {
    const result = hints.error({
      isRateLimited: true,
      retryAfter: 30,
    } as never);
    expect(result[0]).toContain('30s');
  });

  it('error() rate-limited — without retryAfter', () => {
    const result = hints.error({ isRateLimited: true } as never);
    expect(result[0]).toContain('Wait before');
  });

  it('error() status 401', () => {
    const result = hints.error({ status: 401 } as never);
    expect(result[0]).toContain('GITHUB_TOKEN');
  });

  it('error() status 403', () => {
    const result = hints.error({ status: 403 } as never);
    expect(result[0]).toContain('repo');
  });

  it('error() status 404 — with owner+repo', () => {
    const result = hints.error({
      status: 404,
      owner: 'acme',
      repo: 'widget',
    } as never);
    expect(result[0]).toContain("'acme/widget'");
  });

  it('error() status 404 — without owner/repo', () => {
    const result = hints.error({ status: 404 } as never);
    expect(result[0]).toContain('repository');
  });

  it('error() unknown error returns []', () => {
    expect(hints.error({})).toEqual([]);
  });
});

describe('local_fetch_content hints — uncovered branches', () => {
  let hints: (typeof import('../../../octocode-tools-core/src/tools/local_fetch_content/hints.js'))['hints'];

  beforeAll(async () => {
    ({ hints } =
      await import('../../../octocode-tools-core/src/tools/local_fetch_content/hints.js'));
  });

  it('empty() returns [] when path is not a string', () => {
    expect(hints.empty({})).toEqual([]);
  });

  it('empty() returns hints when path provided', () => {
    const result = hints.empty({ path: '/src/foo.ts' } as never);
    expect(result.length).toBeGreaterThan(0);
    expect(result[0]).toContain('localFindFiles');
  });

  it('error() size_limit — with totalLines pushes tail hint', () => {
    const result = hints.error({
      errorType: 'size_limit',
      fileSize: 512000,
      totalLines: 1000,
    } as never);
    expect(result.some(h => !!h && h.includes('Tail:'))).toBe(true);
  });

  it('error() size_limit — without totalLines omits tail hint', () => {
    const result = hints.error({
      errorType: 'size_limit',
      fileSize: 512000,
    } as never);
    expect(result.some(h => !!h && h.includes('total lines'))).toBe(false);
  });

  it('error() not_found — with path', () => {
    const result = hints.error({
      errorType: 'not_found',
      path: '/missing/file.ts',
    } as never);
    expect(result[0]).toContain('localFindFiles');
  });

  it('error() permission', () => {
    const result = hints.error({ errorType: 'permission' } as never);
    expect(result[0]).toContain('Permission denied');
  });

  it('error() unknown returns []', () => {
    expect(hints.error({})).toEqual([]);
  });
});

describe('local_find_files hints — uncovered branches', () => {
  let hints: (typeof import('../../../octocode-tools-core/src/tools/local_find_files/hints.js'))['hints'];

  beforeAll(async () => {
    ({ hints } =
      await import('../../../octocode-tools-core/src/tools/local_find_files/hints.js'));
  });

  it('empty() with sizeLess filter', () => {
    const result = hints.empty({
      name: 'foo.ts',
      sizeLess: '100kb',
    } as never);
    expect(result.length).toBeGreaterThan(0);
    expect(result[0]).toContain('filter');
  });

  it('empty() with no filters returns []', () => {
    expect(hints.empty({})).toEqual([]);
  });

  it('error() not_found — with path', () => {
    const result = hints.error({
      errorType: 'not_found',
      path: '/workspace',
    } as never);
    expect(result[0]).toContain('localViewStructure');
  });

  it('error() not_found — without path', () => {
    const result = hints.error({ errorType: 'not_found' } as never);
    expect(result[0]).toContain('localViewStructure');
  });

  it('error() permission', () => {
    const result = hints.error({ errorType: 'permission' } as never);
    expect(result[0]).toContain('Permission denied');
  });

  it('error() unknown returns []', () => {
    expect(hints.error({})).toEqual([]);
  });
});

describe('local_view_structure hints — uncovered branches', () => {
  let lvsHints: (typeof import('../../../octocode-tools-core/src/tools/local_view_structure/hints.js'))['hints'];

  beforeAll(async () => {
    ({ hints: lvsHints } =
      await import('../../../octocode-tools-core/src/tools/local_view_structure/hints.js'));
  });

  it('error() not_found — with path', () => {
    const result = lvsHints.error({
      errorType: 'not_found',
      path: '/src',
    } as never);
    expect(result[0]).toContain('localFindFiles');
  });

  it('error() not_found — without path', () => {
    const result = lvsHints.error({ errorType: 'not_found' } as never);
    expect(result[0]).toContain('localFindFiles');
  });

  it('error() permission', () => {
    const result = lvsHints.error({ errorType: 'permission' } as never);
    expect(result[0]).toContain('Permission denied');
  });

  it('error() unknown returns []', () => {
    expect(lvsHints.error({})).toEqual([]);
  });

  it('empty() with extensions filter returns hints', () => {
    const result = lvsHints.empty({ extensions: ['ts'] } as never);
    expect(result.length).toBeGreaterThan(0);
    expect(result[0]).toContain('filter');
  });
});

describe('github_search_code hints — single keyword fallback', () => {
  let scHints: (typeof import('../../../octocode-tools-core/src/tools/github_search_code/hints.js'))['hints'];

  beforeAll(async () => {
    ({ hints: scHints } =
      await import('../../../octocode-tools-core/src/tools/github_search_code/hints.js'));
  });

  it('empty() with a single keyword returns a cross-GitHub broadening hint', () => {
    const result = scHints.empty({
      keywords: ['vitest'],
    } as never);
    expect(result.length).toBeGreaterThan(0);
    expect(result.join(' ')).toMatch(/scope|broaden|keywords|owner/i);
  });

  it('empty() with nonExistentScope — owner+repo provided', () => {
    const result = scHints.empty({
      nonExistentScope: true,
      owner: 'acme',
      repo: 'widget',
    } as never);
    expect(result[0]).toContain('acme/widget');
  });
});
