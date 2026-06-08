/**
 * Targeted branch coverage for hints modules that had low branch coverage.
 * Each describe block tests the uncovered branches identified in the v8 report.
 */
import { describe, it, expect } from 'vitest';

// ── github_view_repo_structure/hints.ts (was 60.52%) ────────────────────────

describe('github_view_repo_structure hints — uncovered branches', () => {
  let hints: Awaited<
    ReturnType<
      typeof import('../../src/tools/github_view_repo_structure/hints.js')
    >
  >['hints'];

  beforeAll(async () => {
    ({ hints } =
      await import('../../src/tools/github_view_repo_structure/hints.js'));
  });

  it('empty() returns [] when no path and no branch', () => {
    expect(hints.empty({})).toEqual([]);
  });

  it('empty() returns hints when only path is provided', () => {
    const result = hints.empty({ path: 'src' } as never);
    expect(result.length).toBeGreaterThan(0);
    expect(result[0]).toContain("'src'");
  });

  it('empty() returns hints when only branch is provided', () => {
    const result = hints.empty({ branch: 'main' } as never);
    expect(result.length).toBeGreaterThan(0);
    expect(result[0]).toContain('main');
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

// ── lsp_call_hierarchy/hints.ts (was 59.09%) ────────────────────────────────

describe('lsp_call_hierarchy hints — uncovered branches', () => {
  let hints: Awaited<
    ReturnType<typeof import('../../src/tools/lsp_call_hierarchy/hints.js')>
  >['hints'];

  beforeAll(async () => {
    ({ hints } = await import('../../src/tools/lsp_call_hierarchy/hints.js'));
  });

  it('empty() returns [] when no symbolName', () => {
    expect(hints.empty({})).toEqual([]);
  });

  it('empty() direction=outgoing uses "calls made by" label', () => {
    const result = hints.empty({
      symbolName: 'doWork',
      direction: 'outgoing',
    } as never);
    expect(result[0]).toContain('calls made by');
    expect(result[1]).toContain('may make no calls');
  });

  it('empty() direction=incoming (default) uses "callers of" label', () => {
    const result = hints.empty({ symbolName: 'doWork' } as never);
    expect(result[0]).toContain('callers of');
  });

  it('error() lsp_unavailable — with symbolName', () => {
    const result = hints.error({
      errorType: 'lsp_unavailable',
      symbolName: 'myFn',
    } as never);
    expect(result[1]).toContain('myFn');
  });

  it('error() lsp_unavailable — without symbolName', () => {
    const result = hints.error({ errorType: 'lsp_unavailable' } as never);
    expect(result[1]).toContain('SYMBOL_NAME');
  });

  it('error() not_a_function', () => {
    const result = hints.error({ errorType: 'not_a_function' } as never);
    expect(result[0]).toContain('not a function');
  });

  it('error() timeout — includes depth', () => {
    const result = hints.error({ errorType: 'timeout', depth: 3 } as never);
    expect(result[0]).toContain('Depth=3');
  });

  it('error() unknown returns []', () => {
    expect(hints.error({})).toEqual([]);
  });
});

// ── lsp_find_references/hints.ts (was 62.5%) ────────────────────────────────

describe('lsp_find_references hints — uncovered branches', () => {
  let hints: Awaited<
    ReturnType<typeof import('../../src/tools/lsp_find_references/hints.js')>
  >['hints'];

  beforeAll(async () => {
    ({ hints } = await import('../../src/tools/lsp_find_references/hints.js'));
  });

  it('empty() filteredAll=true', () => {
    const result = hints.empty({ filteredAll: true } as never);
    expect(result[0]).toContain('excluded');
  });

  it('empty() filteredAll=false returns []', () => {
    expect(hints.empty({})).toEqual([]);
  });

  it('error() lsp_unavailable — with symbolName', () => {
    const result = hints.error({
      errorType: 'lsp_unavailable',
      symbolName: 'myFunc',
    } as never);
    expect(result[1]).toContain('myFunc');
  });

  it('error() lsp_unavailable — without symbolName', () => {
    const result = hints.error({ errorType: 'lsp_unavailable' } as never);
    expect(result[1]).toContain('SYMBOL_NAME');
    expect(result[1]).toContain('the symbol');
  });

  it('error() not_found — with symbolName', () => {
    const result = hints.error({
      errorType: 'not_found',
      symbolName: 'myFunc',
    } as never);
    expect(result[0]).toContain('"myFunc"');
  });

  it('error() not_found — without symbolName', () => {
    const result = hints.error({ errorType: 'not_found' } as never);
    expect(result[0]).toContain('LSP could not locate');
  });

  it('error() timeout', () => {
    const result = hints.error({ errorType: 'timeout' } as never);
    expect(result[0]).toContain('timed out');
  });

  it('error() unknown returns []', () => {
    expect(hints.error({})).toEqual([]);
  });
});

// ── local_fetch_content/hints.ts (was 70.83%) ───────────────────────────────

describe('local_fetch_content hints — uncovered branches', () => {
  let hints: Awaited<
    ReturnType<typeof import('../../src/tools/local_fetch_content/hints.js')>
  >['hints'];

  beforeAll(async () => {
    ({ hints } = await import('../../src/tools/local_fetch_content/hints.js'));
  });

  it('empty() returns [] when path is not a string', () => {
    expect(hints.empty({})).toEqual([]);
  });

  it('empty() returns hints when path provided', () => {
    const result = hints.empty({ path: '/src/foo.ts' } as never);
    expect(result[0]).toContain('/src/foo.ts');
  });

  it('error() size_limit — with totalLines pushes tail hint', () => {
    const result = hints.error({
      errorType: 'size_limit',
      fileSize: 512000,
      totalLines: 1000,
    } as never);
    expect(result.some(h => h.includes('total lines'))).toBe(true);
  });

  it('error() size_limit — without totalLines omits tail hint', () => {
    const result = hints.error({
      errorType: 'size_limit',
      fileSize: 512000,
    } as never);
    expect(result.some(h => h.includes('total lines'))).toBe(false);
  });

  it('error() not_found — with path', () => {
    const result = hints.error({
      errorType: 'not_found',
      path: '/missing/file.ts',
    } as never);
    expect(result[0]).toContain('/missing/file.ts');
  });

  it('error() permission', () => {
    const result = hints.error({ errorType: 'permission' } as never);
    expect(result[0]).toContain('Permission denied');
  });

  it('error() unknown returns []', () => {
    expect(hints.error({})).toEqual([]);
  });
});

// ── local_find_files/hints.ts (was 78.12%) ──────────────────────────────────

describe('local_find_files hints — uncovered branches', () => {
  let hints: Awaited<
    ReturnType<typeof import('../../src/tools/local_find_files/hints.js')>
  >['hints'];

  beforeAll(async () => {
    ({ hints } = await import('../../src/tools/local_find_files/hints.js'));
  });

  it('empty() with sizeLess filter', () => {
    const result = hints.empty({
      name: 'foo.ts',
      sizeLess: '100kb',
    } as never);
    expect(result[0]).toContain('sizeLess="100kb"');
  });

  it('empty() with no filters returns []', () => {
    expect(hints.empty({})).toEqual([]);
  });

  it('error() not_found — with path', () => {
    const result = hints.error({
      errorType: 'not_found',
      path: '/workspace',
    } as never);
    expect(result[0]).toContain('/workspace');
  });

  it('error() not_found — without path falls back to "specified"', () => {
    const result = hints.error({ errorType: 'not_found' } as never);
    expect(result[0]).toContain("'specified'");
  });

  it('error() permission', () => {
    const result = hints.error({ errorType: 'permission' } as never);
    expect(result[0]).toContain('Permission denied');
  });

  it('error() unknown returns []', () => {
    expect(hints.error({})).toEqual([]);
  });
});

// ── local_view_structure/hints.ts (was 82.14%) ──────────────────────────────

describe('local_view_structure hints — uncovered branches', () => {
  let lvsHints: Awaited<
    ReturnType<typeof import('../../src/tools/local_view_structure/hints.js')>
  >['hints'];

  beforeAll(async () => {
    ({ hints: lvsHints } = await import(
      '../../src/tools/local_view_structure/hints.js'
    ));
  });

  it('error() not_found — with path', () => {
    const result = lvsHints.error({ errorType: 'not_found', path: '/src' } as never);
    expect(result[0]).toContain('/src');
  });

  it('error() not_found — without path falls back to "specified"', () => {
    const result = lvsHints.error({ errorType: 'not_found' } as never);
    expect(result[0]).toContain('specified');
  });

  it('error() permission', () => {
    const result = lvsHints.error({ errorType: 'permission' } as never);
    expect(result[0]).toContain('Permission denied');
  });

  it('error() unknown returns []', () => {
    expect(lvsHints.error({})).toEqual([]);
  });

  it('empty() with extension filter returns hints', () => {
    const result = lvsHints.empty({ extension: '.ts' } as never);
    expect(result.length).toBeGreaterThan(0);
    expect(result[0]).toContain('.ts');
  });
});

// ── github_search_code/hints.ts line 27 — looksLikeTestInfrastructureQuery ─

describe('github_search_code hints — test infrastructure query detection', () => {
  let scHints: Awaited<
    ReturnType<typeof import('../../src/tools/github_search_code/hints.js')>
  >['hints'];

  beforeAll(async () => {
    ({ hints: scHints } = await import(
      '../../src/tools/github_search_code/hints.js'
    ));
  });

  it('empty() flags test-infrastructure keywords (jest/vitest) in the suggestion', () => {
    // 'vitest' is in TEST_INFRA_KEYWORDS — triggers looksLikeTestInfrastructureQuery
    const result = scHints.empty({
      keywords: ['vitest'],
    } as never);
    const joined = result.join(' ');
    expect(joined.toLowerCase()).toContain('test');
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

// ── lsp_goto_definition/hints.ts (line 15 — symbolName branch in empty()) ──

describe('lsp_goto_definition hints — uncovered branch', () => {
  let hints: Awaited<
    ReturnType<typeof import('../../src/tools/lsp_goto_definition/hints.js')>
  >['hints'];

  beforeAll(async () => {
    ({ hints } = await import('../../src/tools/lsp_goto_definition/hints.js'));
  });

  it('empty() with symbolName but no searchRadius returns definition-not-found hint', () => {
    const result = hints.empty({
      symbolName: 'myFunction',
      uri: 'file:///src/foo.ts',
    } as never);
    expect(result[0]).toContain('"myFunction"');
    expect(result[0]).toContain('Definition not found');
  });

  it('empty() with no symbolName and no searchRadius returns []', () => {
    expect(hints.empty({})).toEqual([]);
  });
});
