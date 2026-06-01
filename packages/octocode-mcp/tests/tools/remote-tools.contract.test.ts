/**
 * Contract + agentic-quality tests for the 6 remote GitHub/package tools.
 *
 * Verifies the post-refactor response contract:
 *  - `format: "tsv" | "json"` (default tsv)
 *  - TSV mode: per-tool columns/rows projection, RFC-style escapes
 *  - Peer-level hints (deduped, lifted out of `data.hints`)
 *  - Dynamic-only hints: pagination, failures, empty-with-context
 *  - No upstream static-guidance strings reach responses
 *
 * Includes a final "agentic quality" scorecard that rates each tool's
 * response on concrete agentic-flow criteria: parseable structure,
 * absence of static noise, actionable hints, token shape.
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { tsvFormat } from '../../src/utils/response/tsvFormat.js';
import { getTsvProjection } from '../../src/utils/response/tsvColumns.js';
import { getHints } from '../../src/hints/index.js';
import { STATIC_TOOL_NAMES } from '../../src/tools/toolNames.js';
import { initializeToolMetadata } from '../../src/tools/toolMetadata/state.js';
import { applyGithubSearchCodeVerbosity } from '../../src/tools/github_search_code/finalizer.js';
import { buildGithubFetchContentFinalizer } from '../../src/tools/github_fetch_content/finalizer.js';
import { applyGithubViewRepoStructureVerbosity } from '../../src/tools/github_view_repo_structure/execution.js';

beforeAll(async () => {
  await initializeToolMetadata();
});

// ===========================================================================
// 1. tsvFormat() — escape correctness, header-only, missing cells
// ===========================================================================

describe('tsvFormat', () => {
  it('returns just the header when there are no rows', () => {
    expect(tsvFormat(['a', 'b'], [])).toBe('a\tb');
  });

  it('joins rows under the header in column order', () => {
    expect(
      tsvFormat(
        ['name', 'age'],
        [
          { name: 'ada', age: 36 },
          { name: 'guy', age: 9000 },
        ]
      )
    ).toBe('name\tage\nada\t36\nguy\t9000');
  });

  it('escapes tab, newline, carriage return, and backslash', () => {
    const out = tsvFormat(['code'], [{ code: 'a\tb\nc\rd\\e' }]);
    // Header on first line, payload on second — every special char escaped.
    expect(out).toBe('code\na\\tb\\nc\\rd\\\\e');
    expect(out.split('\n')).toHaveLength(2);
  });

  it('renders missing cells as empty (not "undefined")', () => {
    const out = tsvFormat(['a', 'b', 'c'], [{ a: 1, c: 3 }]);
    expect(out).toBe('a\tb\tc\n1\t\t3');
  });

  it('JSON-stringifies object / array cells (so TSV remains 1 row per record)', () => {
    const out = tsvFormat(
      ['k', 'v'],
      [
        { k: 'tags', v: ['x', 'y'] },
        { k: 'meta', v: { n: 1 } },
      ]
    );
    const lines = out.split('\n');
    expect(lines).toHaveLength(3);
    expect(lines[1]).toBe('tags\t["x","y"]');
    expect(lines[2]).toBe('meta\t{"n":1}');
  });

  it('keeps booleans and numbers as bare strings (no quoting)', () => {
    expect(tsvFormat(['ok', 'n'], [{ ok: true, n: 42 }])).toBe(
      'ok\tn\ntrue\t42'
    );
  });
});

// ===========================================================================
// 2. TSV column projections — one suite per remote tool
// ===========================================================================

function projectAndFormat(
  tool: string,
  data: unknown
): {
  columns: readonly string[];
  rows: ReadonlyArray<Record<string, unknown>>;
  text: string;
} {
  const projection = getTsvProjection(tool);
  if (!projection) throw new Error(`no projection for ${tool}`);
  const rows = projection.toRows(data);
  return {
    columns: projection.columns,
    rows,
    text: tsvFormat(projection.columns, rows),
  };
}

describe('TSV projection: githubSearchCode', () => {
  it('flattens owner/repo groups to one row per match', () => {
    const { columns, rows, text } = projectAndFormat(
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      {
        results: [
          {
            owner: 'modelcontextprotocol',
            repo: 'typescript-sdk',
            matches: [
              { path: 'src/a.ts', value: 'export class A {}' },
              { path: 'src/b.ts', value: 'export class B {}' },
            ],
          },
        ],
      }
    );
    expect(columns).toEqual(['owner', 'repo', 'path', 'value']);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toMatchObject({
      owner: 'modelcontextprotocol',
      repo: 'typescript-sdk',
      path: 'src/a.ts',
      value: 'export class A {}',
    });
    expect(text).toContain('modelcontextprotocol\ttypescript-sdk\tsrc/a.ts');
  });

  it('omits params and payload cells without crashing', () => {
    const { rows, columns } = projectAndFormat(
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      {
        results: [
          {
            owner: 'o',
            repo: 'r',
            matches: [{ path: 'x.ts' }],
          },
        ],
      }
    );
    expect(columns).toEqual(['owner', 'repo', 'path', 'value']);
    expect(rows).toEqual([{ owner: 'o', repo: 'r', path: 'x.ts', value: '' }]);
  });
});

describe('Verbosity: githubSearchCode', () => {
  it('keeps a one-line snippet and dedupes duplicate paths in concise mode', () => {
    const responseData = {
      results: [
        {
          id: 'facebook/react',
          owner: 'facebook',
          repo: 'react',
          matches: [
            {
              path: 'ReactFiberThrow.js',
              value: 'function throwException() {',
            },
            { path: 'ReactFiberThrow.js', value: 'throw value;' },
            {
              path: 'ReactFiberHooks.js',
              value: '\n  export function useState() {\n    return null;\n  }',
            },
          ],
        },
      ],
    };

    expect(
      applyGithubSearchCodeVerbosity(responseData, [{ verbosity: 'concise' }])
    ).toBe(true);

    expect(responseData.results[0]!.matches).toEqual([
      { path: 'ReactFiberThrow.js', value: 'function throwException() {' },
      { path: 'ReactFiberHooks.js', value: 'export function useState() {' },
    ]);
  });
});

describe('TSV projection: githubGetFileContent', () => {
  it('emits one row per file with pagination/author metadata', () => {
    const { columns, rows } = projectAndFormat(
      STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
      {
        results: [
          {
            owner: 'a',
            repo: 'b',
            files: [
              {
                path: 'README.md',
                content: '# hi',
                startLine: 1,
                endLine: 40,
                totalLines: 200,
                lastModifiedBy: 'someone',
              },
            ],
          },
        ],
      }
    );
    // `content` intentionally omitted from TSV columns — file body lives in
    // JSON `data.results[].files[].content` only. TSV is the metadata view.
    expect(columns).toEqual([
      'owner',
      'repo',
      'path',
      'totalLines',
      'resolvedBranch',
      'isPartial',
      'startLine',
      'endLine',
      'lastModified',
      'lastModifiedBy',
      'warnings',
      'localPath',
      'fileCount',
      'totalSize',
      'size',
      'type',
      'cached',
    ]);
    expect(columns).not.toContain('content');
    expect(rows[0]).toMatchObject({
      owner: 'a',
      repo: 'b',
      path: 'README.md',
      startLine: 1,
      endLine: 40,
      totalLines: 200,
      lastModifiedBy: 'someone',
    });
  });

  it('omits file content from TSV rows', () => {
    const { text, columns } = projectAndFormat(
      STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
      {
        results: [
          {
            owner: 'a',
            repo: 'b',
            files: [{ path: 'x.ts', content: 'line1\nline2\nline3' }],
          },
        ],
      }
    );
    // File content lives in JSON `data.results[].files[].content`; TSV is
    // a metadata view, so neither the column nor any row carries the body.
    expect(columns).not.toContain('content');
    expect(text).not.toContain('line1');
    expect(text).not.toContain('line2');
    expect(text).not.toContain('line3');
  });
});

describe('Evidence: githubGetFileContent', () => {
  it('nudges the next pagination parameter for partial file content', () => {
    const finalizer = buildGithubFetchContentFinalizer();
    const output = finalizer({
      queries: [
        {
          owner: 'o',
          repo: 'r',
          path: 'src/a.ts',
        },
      ],
      results: [
        {
          id: 'q1',
          data: {
            path: 'src/a.ts',
            content: 'partial',
            isPartial: true,
            totalLines: 120,
            startLine: 1,
            endLine: 40,
            pagination: {
              currentPage: 1,
              totalPages: 3,
              hasMore: true,
              charOffset: 0,
              charLength: 200,
              totalChars: 600,
            },
          },
        },
      ],
      config: {
        toolName: STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
        format: 'json',
        peerEvidence: true,
      },
    });

    expect(output.structuredContent.evidence?.reason).toContain(
      'Use charOffset=200 for o/r:src/a.ts.'
    );
    expect(output.structuredContent.evidence?.reason).toContain(
      'Use startLine=41 with an endLine up to 120 for o/r:src/a.ts.'
    );
  });
});

describe('TSV projection: githubSearchRepositories', () => {
  it('emits one row per repository with topics space-joined', () => {
    const { columns, rows } = projectAndFormat(
      STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
      {
        repositories: [
          {
            owner: 'o',
            repo: 'r',
            stars: 1234,
            language: 'TypeScript',
            pushedAt: '2026-05-23',
            forksCount: 10,
            openIssuesCount: 5,
            topics: ['mcp', 'agents'],
            description: 'Hello',
          },
        ],
      }
    );
    expect(columns).toContain('topics');
    expect(columns).toContain('description');
    expect(rows[0]).toMatchObject({
      repo: 'r',
      language: 'TypeScript',
      topics: ['mcp', 'agents'],
      description: 'Hello',
    });
  });
});

describe('TSV projection: githubSearchPullRequests', () => {
  it('flattens PR entries; nested fileChanges intentionally omitted', () => {
    const { columns, rows } = projectAndFormat(
      STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      {
        pull_requests: [
          {
            number: 2147,
            state: 'open',
            author: 'alice',
            title: 'feat: x',
            createdAt: '2026-05-01',
            updatedAt: '2026-05-23',
            additions: 100,
            deletions: 20,
            changedFilesCount: 3,
            url: 'https://github.com/o/r/pull/2147',
            fileChanges: [{ path: 'a', additions: 1, deletions: 0 }],
          },
        ],
      }
    );
    expect(columns).toContain('fileChanges');
    expect(columns).toContain('body');
    expect(columns).toContain('title');
    expect(columns).toContain('comments');
    expect(columns).toContain('state');
    expect(columns).toContain('author');
    expect(rows[0]).toMatchObject({
      number: 2147,
      state: 'open',
      author: 'alice',
      title: 'feat: x',
      additions: 100,
      deletions: 20,
      changedFilesCount: 3,
      fileChanges: [{ path: 'a', additions: 1, deletions: 0 }],
    });
  });
});

describe('Verbosity: githubViewRepoStructure', () => {
  it('suggests concrete next paths when a structure response is truncated', () => {
    const shaped = applyGithubViewRepoStructureVerbosity(
      {
        data: {
          path: '',
          structure: {
            '.': {
              folders: ['packages', 'docs'],
              files: ['README.md'],
            },
          },
        },
        entryCount: 3,
        summary: { truncated: true },
        extraHints: [],
      },
      { verbosity: 'basic' }
    );

    expect(shaped.extraHints).toContain(
      'Next paths: packages/, docs/, README.md'
    );
  });
});

describe('TSV projection: githubViewRepoStructure', () => {
  it('flattens the nested tree to (path, type) rows', () => {
    const { columns, rows } = projectAndFormat(
      STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      {
        structure: {
          '.': { files: ['README.md', 'package.json'], folders: ['src'] },
          src: { files: ['index.ts'], folders: [] },
        },
      }
    );
    // Lean projection: 2 useful columns. `parent`/`name` were dropped —
    // `path` (= parent/name) is the chainable full path and basename/dirname
    // are derivable from it, so emitting all three duplicated every row.
    // `size`/`sha`/`url` were dropped earlier — the structure payload carries
    // no data for them.
    expect(columns).toEqual(['path', 'type']);
    expect(rows).toContainEqual({
      type: 'file',
      path: 'README.md',
    });
    expect(rows).toContainEqual({
      type: 'dir',
      path: 'src',
    });
    expect(rows).toContainEqual({
      type: 'file',
      path: 'src/index.ts',
    });
  });

  it('emits zero rows on empty structure (header still rendered)', () => {
    const { text, rows } = projectAndFormat(
      STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      { structure: {} }
    );
    expect(rows).toHaveLength(0);
    expect(text).toBe('path\ttype');
  });
});

describe('TSV projection: packageSearch', () => {
  it('emits one row per package with downloads + license metadata', () => {
    const { rows } = projectAndFormat(STATIC_TOOL_NAMES.PACKAGE_SEARCH, {
      packages: [
        {
          name: 'react',
          version: '19.0.0',
          owner: 'facebook',
          repo: 'react',
          weeklyDownloads: 50000000,
          lastPublished: '2026-04-01',
          license: 'MIT',
          description: 'A JavaScript library for building user interfaces',
        },
      ],
    });
    expect(rows[0]).toMatchObject({
      name: 'react',
      version: '19.0.0',
      owner: 'facebook',
      weeklyDownloads: 50000000,
      license: 'MIT',
    });
  });
});

// ===========================================================================
// 3. Dynamic-only hints — assert no static guidance strings leak
// ===========================================================================

const FORBIDDEN_STATIC_PHRASES = [
  "Use 'owner', 'repo'",
  "Follow 'mainResearchGoal'",
  'Do findings answer your question',
  'Got 3+ examples',
  'Check timestamps (pushedAt, lastModified)',
  'Check DEPRECATED warnings',
  'Next: githubViewRepoStructure',
  'Then: githubSearchCode',
  'OUTPUT: Use owner, name',
  'Drill deeper: depth=2',
  'TO GET NEXT PAGE',
  '📂',
  '📊',
];

describe('hints contract — static guidance never reaches responses', () => {
  const remoteTools = [
    STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
    STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
    STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    STATIC_TOOL_NAMES.PACKAGE_SEARCH,
  ];

  for (const tool of remoteTools) {
    for (const status of [undefined, 'empty', 'error'] as const) {
      it(`${tool} (${status}) — no static guidance phrases`, () => {
        const hints = getHints(tool, status, { hasOwnerRepo: false });
        for (const phrase of FORBIDDEN_STATIC_PHRASES) {
          for (const hint of hints) {
            expect(hint).not.toContain(phrase);
          }
        }
      });
    }
  }

  it('githubSearchCode error with rate limit emits a conditional retry hint', () => {
    const hints = getHints(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE, 'error', {
      isRateLimited: true,
      retryAfter: 30,
    });
    expect(hints.some(h => h.includes('Retry after 30s'))).toBe(true);
  });

  it('githubSearchCode empty names the scope when owner/repo set', () => {
    const hints = getHints(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE, 'empty', {
      hasOwnerRepo: true,
      owner: 'a',
      repo: 'b',
    });
    expect(hints.some(h => h.includes('a/b'))).toBe(true);
  });

  it('per-tool hints fire only on empty/error — hasResults channel is type-narrowed away', () => {
    // The HintStatus type no longer includes 'hasResults'. Pagination signals
    // live in the response envelope (pagination/hints from executor extraHints).
    const emptyHints = getHints(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE, 'empty', {
      hasOwnerRepo: true,
      owner: 'a',
      repo: 'b',
    });
    expect(emptyHints.length).toBeGreaterThan(0);
  });

  it('githubGetFileContent error not_found emits path-aware recovery', () => {
    const hints = getHints(STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT, 'error', {
      errorType: 'not_found',
      path: 'src/foo.ts',
      branch: 'main',
    });
    expect(hints.some(h => h.includes('src/foo.ts'))).toBe(true);
  });
});

// ===========================================================================
// 4. Agentic quality scorecards — rate each tool's response on concrete
//    criteria that matter for downstream agent loops. Each criterion is
//    binary (pass=1, fail=0). A tool passes overall if it scores >= 4/5.
// ===========================================================================

type Scorecard = {
  noStaticNoise: boolean;
  hintsAreActionable: boolean;
  hintsArePeerLevel: boolean;
  tsvIsParseable: boolean;
  paginationCursorWhenMore: boolean;
};

function rateAgenticQuality(card: Scorecard): {
  score: number;
  failures: string[];
} {
  const failures: string[] = [];
  for (const [k, v] of Object.entries(card)) {
    if (!v) failures.push(k);
  }
  return { score: 5 - failures.length, failures };
}

function buildScorecard(
  tool: string,
  sample: {
    data: Record<string, unknown>;
    hints: string[];
    hasMore: boolean;
    paginationHint?: string;
  }
): Scorecard {
  // 1. No static guidance noise.
  const noStaticNoise = !sample.hints.some(h =>
    FORBIDDEN_STATIC_PHRASES.some(p => h.includes(p))
  );

  // 2. Hints are actionable — each one names a concrete parameter, value,
  //    error code, or page number. Pure prose like "Consider..." fails.
  const ACTIONABLE_MARKERS = [
    /\bpage=\d/,
    /\bstartLine=\d/,
    /\bcharOffset=\d/,
    /\bmatch=/,
    /Retry after \d+s/,
    /across repos/,
    /Permission denied/,
    /\bGITHUB_TOKEN\b/,
    /Partial content/,
    /entries\)/,
    /Page \d+\/\d+/, // "Page 1/3 (...)" pagination summary
  ];
  const hintsAreActionable =
    sample.hints.length === 0 ||
    sample.hints.every(h => ACTIONABLE_MARKERS.some(re => re.test(h)));

  // 3. Hints emitted at peer level (not nested inside data).
  const hintsArePeerLevel =
    !('hints' in sample.data) ||
    (Array.isArray((sample.data as Record<string, unknown>).hints) &&
      ((sample.data as { hints: unknown[] }).hints as unknown[]).length === 0);

  // 4. TSV parseability: header + N rows, each row has |columns| tab-separated
  //    cells. Embedded newlines inside content cells must be escaped to \n
  //    so the row count equals (#records + 1 header).
  const projection = getTsvProjection(tool);
  let tsvIsParseable = true;
  if (projection) {
    const rows = projection.toRows(sample.data);
    const text = tsvFormat(projection.columns, rows);
    const lines = text.split('\n');
    tsvIsParseable =
      projection.columns.length === 0
        ? text === '' && rows.length === 0
        : lines.length === rows.length + 1 &&
          lines.every(l => l.split('\t').length === projection.columns.length);
  }

  // 5. When more results exist, a pagination cursor hint is provided.
  const paginationCursorWhenMore =
    !sample.hasMore ||
    (sample.paginationHint !== undefined && /=\d/.test(sample.paginationHint));

  return {
    noStaticNoise,
    hintsAreActionable,
    hintsArePeerLevel,
    tsvIsParseable,
    paginationCursorWhenMore,
  };
}

describe('agentic-flow quality scorecards', () => {
  it('githubSearchCode: clean response scores 5/5', () => {
    const card = buildScorecard(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE, {
      data: {
        results: [
          {
            owner: 'o',
            repo: 'r',
            matches: [{ path: 'a.ts', value: 'line1\nline2' }],
          },
        ],
      },
      hints: ['Page 1/3 (1-10 of 30)', 'Next: page=2'],
      hasMore: true,
      paginationHint: 'Next: page=2',
    });
    const { score, failures } = rateAgenticQuality(card);
    expect(failures).toEqual([]);
    expect(score).toBe(5);
  });

  it('githubGetFileContent: response with embedded newlines still parses', () => {
    const card = buildScorecard(STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT, {
      data: {
        results: [
          {
            owner: 'a',
            repo: 'b',
            files: [
              { path: 'x.ts', content: 'a\nb\nc', startLine: 1, endLine: 3 },
            ],
          },
        ],
      },
      hints: ['Partial content ends at line 3. Use startLine=4 to continue.'],
      hasMore: false,
    });
    expect(rateAgenticQuality(card).score).toBe(5);
  });

  it('githubSearchRepositories: 5/5 with a peer pagination hint', () => {
    const card = buildScorecard(STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES, {
      data: {
        repositories: [{ owner: 'o', repo: 'r', stars: 1, topics: ['x'] }],
      },
      hints: ['Next: page=2'],
      hasMore: true,
      paginationHint: 'Next: page=2',
    });
    expect(rateAgenticQuality(card).failures).toEqual([]);
  });

  it('githubSearchPullRequests: scorecard with body+fileChanges excluded from TSV', () => {
    const card = buildScorecard(STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, {
      data: {
        pull_requests: [
          {
            number: 1,
            state: 'open',
            author: 'a',
            title: 't',
            additions: 1,
            deletions: 0,
            changedFilesCount: 1,
            url: 'u',
          },
        ],
      },
      hints: [],
      hasMore: false,
    });
    expect(rateAgenticQuality(card).score).toBe(5);
  });

  it('githubViewRepoStructure: scorecard for nested tree', () => {
    const card = buildScorecard(STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE, {
      data: {
        structure: {
          '.': { files: ['a'], folders: ['src'] },
          src: { files: ['b'], folders: [] },
        },
      },
      hints: [],
      hasMore: false,
    });
    expect(rateAgenticQuality(card).score).toBe(5);
  });

  it('packageSearch: scorecard with empty rows still scores parseable', () => {
    const card = buildScorecard(STATIC_TOOL_NAMES.PACKAGE_SEARCH, {
      data: { packages: [] },
      hints: [],
      hasMore: false,
    });
    expect(rateAgenticQuality(card).score).toBe(5);
  });

  it('FAILS when a static guidance phrase sneaks into hints', () => {
    const card = buildScorecard(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE, {
      data: { results: [] },
      hints: ['Got 3+ examples? Consider stopping to avoid over-research'],
      hasMore: false,
    });
    const { score, failures } = rateAgenticQuality(card);
    expect(failures).toContain('noStaticNoise');
    expect(score).toBeLessThan(5);
  });

  it('FAILS when hints are nested inside data (not peer-level)', () => {
    const card = buildScorecard(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE, {
      data: {
        results: [],
        hints: ['Page 1/2'],
      },
      hints: ['Page 1/2'],
      hasMore: true,
      paginationHint: 'Next: page=2',
    });
    expect(rateAgenticQuality(card).failures).toContain('hintsArePeerLevel');
  });

  it('FAILS when hasMore but no pagination cursor hint is provided', () => {
    const card = buildScorecard(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE, {
      data: { results: [] },
      hints: [],
      hasMore: true,
    });
    expect(rateAgenticQuality(card).failures).toContain(
      'paginationCursorWhenMore'
    );
  });

  it('FAILS when a hint is prose without an actionable token', () => {
    const card = buildScorecard(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE, {
      data: { results: [] },
      hints: ['Consider trying again later.'],
      hasMore: false,
    });
    expect(rateAgenticQuality(card).failures).toContain('hintsAreActionable');
  });
});
