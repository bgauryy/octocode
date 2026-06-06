import { describe, it, expect, beforeAll } from 'vitest';
import { getHints } from '../../src/hints/index.js';
import { STATIC_TOOL_NAMES } from '../../src/tools/toolNames.js';
import { initializeToolMetadata } from '../../src/tools/toolMetadata/state.js';
import { applyGithubSearchCodeVerbosity } from '../../src/tools/github_search_code/finalizer.js';
import { buildGithubFetchContentFinalizer } from '../../src/tools/github_fetch_content/finalizer.js';
import { applyGithubViewRepoStructureVerbosity } from '../../src/tools/github_view_repo_structure/execution.js';

beforeAll(async () => {
  await initializeToolMetadata();
});

describe('Verbosity: githubSearchCode', () => {
  it('verbose=false strips matchIndices metadata, preserves core match data', () => {
    const originalMatches = [
      {
        path: 'ReactFiberThrow.js',
        value: 'function throwException() {',
        matchIndices: [{ start: 0, end: 5 }],
      },
      { path: 'ReactFiberThrow.js', value: 'throw value;' },
    ];
    const responseData = {
      results: [
        {
          id: 'facebook/react',
          owner: 'facebook',
          repo: 'react',
          matches: [...originalMatches],
        },
      ],
    };

    applyGithubSearchCodeVerbosity(responseData, [{ verbose: false }]);
    expect(responseData.results[0]!.matches[0]).not.toHaveProperty(
      'matchIndices'
    );
    expect(responseData.results[0]!.matches[0]!.value).toBe(
      originalMatches[0]!.value
    );
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
        peerEvidence: true,
      },
    });

    expect(output.structuredContent.evidence?.reason).toContain(
      'Use charOffset=200 for o/r:src/a.ts.'
    );
    const hints = output.structuredContent.hints as string[] | undefined;
    expect(hints?.some(h => h.includes('startLine=41'))).toBe(true);
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
      { verbose: false }
    );

    expect(shaped.extraHints).toContain(
      'Next paths: packages/, docs/, README.md'
    );
  });
});

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

type Scorecard = {
  noStaticNoise: boolean;
  hintsAreActionable: boolean;
  hintsArePeerLevel: boolean;
  paginationCursorWhenMore: boolean;
};

const MAX_SCORE = 4;

function rateAgenticQuality(card: Scorecard): {
  score: number;
  failures: string[];
} {
  const failures: string[] = [];
  for (const [k, v] of Object.entries(card)) {
    if (!v) failures.push(k);
  }
  return { score: MAX_SCORE - failures.length, failures };
}

function buildScorecard(sample: {
  data: Record<string, unknown>;
  hints: string[];
  hasMore: boolean;
  paginationHint?: string;
}): Scorecard {
  const noStaticNoise = !sample.hints.some(h =>
    FORBIDDEN_STATIC_PHRASES.some(p => h.includes(p))
  );

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
    /Page \d+\/\d+/,
  ];
  const hintsAreActionable =
    sample.hints.length === 0 ||
    sample.hints.every(h => ACTIONABLE_MARKERS.some(re => re.test(h)));

  const hintsArePeerLevel =
    !('hints' in sample.data) ||
    (Array.isArray((sample.data as Record<string, unknown>).hints) &&
      ((sample.data as { hints: unknown[] }).hints as unknown[]).length === 0);

  const paginationCursorWhenMore =
    !sample.hasMore ||
    (sample.paginationHint !== undefined && /=\d/.test(sample.paginationHint));

  return {
    noStaticNoise,
    hintsAreActionable,
    hintsArePeerLevel,
    paginationCursorWhenMore,
  };
}

describe('agentic-flow quality scorecards', () => {
  it('githubSearchCode: clean response scores full marks', () => {
    const card = buildScorecard({
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
    expect(score).toBe(MAX_SCORE);
  });

  it('githubGetFileContent: response with a continuation hint scores full marks', () => {
    const card = buildScorecard({
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
    expect(rateAgenticQuality(card).score).toBe(MAX_SCORE);
  });

  it('githubSearchRepositories: full marks with a peer pagination hint', () => {
    const card = buildScorecard({
      data: {
        repositories: [{ owner: 'o', repo: 'r', stars: 1, topics: ['x'] }],
      },
      hints: ['Next: page=2'],
      hasMore: true,
      paginationHint: 'Next: page=2',
    });
    expect(rateAgenticQuality(card).failures).toEqual([]);
  });

  it('githubSearchPullRequests: clean scorecard', () => {
    const card = buildScorecard({
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
    expect(rateAgenticQuality(card).score).toBe(MAX_SCORE);
  });

  it('githubViewRepoStructure: scorecard for nested tree', () => {
    const card = buildScorecard({
      data: {
        structure: {
          '.': { files: ['a'], folders: ['src'] },
          src: { files: ['b'], folders: [] },
        },
      },
      hints: [],
      hasMore: false,
    });
    expect(rateAgenticQuality(card).score).toBe(MAX_SCORE);
  });

  it('packageSearch: scorecard with empty rows still scores full marks', () => {
    const card = buildScorecard({
      data: { packages: [] },
      hints: [],
      hasMore: false,
    });
    expect(rateAgenticQuality(card).score).toBe(MAX_SCORE);
  });

  it('FAILS when a static guidance phrase sneaks into hints', () => {
    const card = buildScorecard({
      data: { results: [] },
      hints: ['Got 3+ examples? Consider stopping to avoid over-research'],
      hasMore: false,
    });
    const { score, failures } = rateAgenticQuality(card);
    expect(failures).toContain('noStaticNoise');
    expect(score).toBeLessThan(MAX_SCORE);
  });

  it('FAILS when hints are nested inside data (not peer-level)', () => {
    const card = buildScorecard({
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
    const card = buildScorecard({
      data: { results: [] },
      hints: [],
      hasMore: true,
    });
    expect(rateAgenticQuality(card).failures).toContain(
      'paginationCursorWhenMore'
    );
  });

  it('FAILS when a hint is prose without an actionable token', () => {
    const card = buildScorecard({
      data: { results: [] },
      hints: ['Consider trying again later.'],
      hasMore: false,
    });
    expect(rateAgenticQuality(card).failures).toContain('hintsAreActionable');
  });
});
