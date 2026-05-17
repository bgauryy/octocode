import { EventEmitter } from 'events';
import { readFileSync } from 'fs';
import path from 'path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  LocalGetFileContentDataSchema,
  LocalGetFileContentOutputSchema,
  LocalSearchCodeDataSchema,
  LocalSearchCodeOutputSchema,
  LspCallHierarchyDataSchema,
  LspCallHierarchyOutputSchema,
  LspFindReferencesDataSchema,
  LspFindReferencesOutputSchema,
} from '@octocodeai/octocode-core';
import { FLOW_CATALOG } from './catalog.js';
import {
  createFlowHarness,
  getFlowFixturePath,
  localResearchFlowTools,
} from './harness.js';
import { expectHasResultsData } from './assertions.js';

const flowRuntime = vi.hoisted(() => ({
  safeExec: vi.fn(),
  checkCommandAvailability: vi.fn(),
  spawn: vi.fn(),
}));

vi.mock('../../src/utils/exec/safe.js', async () => {
  const actual = await vi.importActual<
    typeof import('../../src/utils/exec/safe.js')
  >('../../src/utils/exec/safe.js');

  return {
    ...actual,
    safeExec: flowRuntime.safeExec,
  };
});

vi.mock('../../src/utils/exec/commandAvailability.js', async () => {
  const actual = await vi.importActual<
    typeof import('../../src/utils/exec/commandAvailability.js')
  >('../../src/utils/exec/commandAvailability.js');

  return {
    ...actual,
    checkCommandAvailability: flowRuntime.checkCommandAvailability,
  };
});

vi.mock('child_process', () => {
  return {
    spawn: flowRuntime.spawn,
  };
});

vi.mock('../../src/lsp/manager.js', async () => {
  const actual = await vi.importActual<
    typeof import('../../src/lsp/manager.js')
  >('../../src/lsp/manager.js');

  return {
    ...actual,
    acquirePooledClient: vi.fn().mockResolvedValue(null),
    isLanguageServerAvailable: vi.fn().mockResolvedValue(false),
  };
});

vi.mock(
  '../../src/tools/lsp_find_references/lspReferencesPatterns.js',
  async () => {
    const actual = await vi.importActual<
      typeof import('../../src/tools/lsp_find_references/lspReferencesPatterns.js')
    >('../../src/tools/lsp_find_references/lspReferencesPatterns.js');

    return {
      ...actual,
      findReferencesWithPatternMatching: vi.fn(
        async (_absolutePath, _root, query) => {
          const allLocations = [
            {
              uri: `${process.env.WORKSPACE_ROOT}/src/consumer.ts`,
              range: {
                start: { line: 3, character: 16 },
                end: { line: 3, character: 28 },
              },
              content: '  const score = computeScore(input);',
              isDefinition: false,
            },
            {
              uri: `${process.env.WORKSPACE_ROOT}/src/secondary.ts`,
              range: {
                start: { line: 6, character: 9 },
                end: { line: 6, character: 21 },
              },
              content: '  return computeScore(left) - computeScore(right);',
              isDefinition: false,
            },
            {
              uri: `${process.env.WORKSPACE_ROOT}/src/score.ts`,
              range: {
                start: { line: 14, character: 18 },
                end: { line: 14, character: 30 },
              },
              content: '  return `score:${computeScore(input)}`;',
              isDefinition: false,
            },
            {
              uri: `${process.env.WORKSPACE_ROOT}/src/score.ts`,
              range: {
                start: { line: 9, character: 16 },
                end: { line: 9, character: 28 },
              },
              content:
                'export function computeScore(input: ScoreInput): number {',
              isDefinition: true,
            },
          ];

          const filteredLocations = query.includeDeclaration
            ? allLocations
            : allLocations.filter(location => !location.isDefinition);
          const referencesPerPage = query.referencesPerPage ?? 20;
          const page = query.page ?? 1;
          const totalPages = Math.ceil(
            filteredLocations.length / referencesPerPage
          );
          const startIndex = (page - 1) * referencesPerPage;
          const paginatedLocations = filteredLocations.slice(
            startIndex,
            startIndex + referencesPerPage
          );
          const hasMultipleFiles =
            new Set(filteredLocations.map(location => location.uri)).size > 1;

          if (paginatedLocations.length === 0) {
            return {
              status: 'empty',
              hints: [`No references found for '${query.symbolName}'`],
            };
          }

          return {
            status: 'hasResults',
            locations: paginatedLocations,
            pagination: {
              currentPage: page,
              totalPages,
              totalResults: filteredLocations.length,
              hasMore: page < totalPages,
              resultsPerPage: referencesPerPage,
            },
            hasMultipleFiles,
            hints: [
              `Found ${filteredLocations.length} reference(s) using text search`,
            ],
          };
        }
      ),
    };
  }
);

vi.mock(
  '../../src/tools/lsp_call_hierarchy/callHierarchyPatterns.js',
  async () => {
    const actual = await vi.importActual<
      typeof import('../../src/tools/lsp_call_hierarchy/callHierarchyPatterns.js')
    >('../../src/tools/lsp_call_hierarchy/callHierarchyPatterns.js');

    return {
      ...actual,
      callHierarchyWithPatternMatching: vi.fn(async (query, absolutePath) => ({
        status: 'hasResults',
        item: {
          name: query.symbolName,
          kind: 'function',
          uri: absolutePath,
          range: {
            start: { line: 9, character: 16 },
            end: { line: 9, character: 28 },
          },
          content: 'export function computeScore(input: ScoreInput): number {',
        },
        direction: query.direction,
        depth: query.depth ?? 1,
        incomingCalls: [
          {
            from: {
              name: 'buildSummary',
              kind: 'function',
              uri: `${process.env.WORKSPACE_ROOT}/src/consumer.ts`,
              range: {
                start: { line: 2, character: 16 },
                end: { line: 2, character: 28 },
              },
              content:
                'export function buildSummary(input: ScoreInput): string {',
            },
            fromRanges: [
              {
                start: { line: 3, character: 16 },
                end: { line: 3, character: 28 },
              },
            ],
          },
          {
            from: {
              name: 'compareScores',
              kind: 'function',
              uri: `${process.env.WORKSPACE_ROOT}/src/secondary.ts`,
              range: {
                start: { line: 2, character: 16 },
                end: { line: 2, character: 29 },
              },
              content: 'export function compareScores(',
            },
            fromRanges: [
              {
                start: { line: 6, character: 9 },
                end: { line: 6, character: 21 },
              },
            ],
          },
        ],
        pagination: {
          currentPage: 1,
          totalPages: 1,
          totalResults: 2,
          hasMore: false,
          resultsPerPage: query.callsPerPage ?? 15,
        },
        hints: ['Use localGetFileContent to inspect one caller in detail'],
      })),
    };
  }
);

describe(FLOW_CATALOG.localImpactAnalysis.id, () => {
  const fixtureRepoPath = getFlowFixturePath('mini-ts-repo');
  const fixtureSourcePath = `${fixtureRepoPath}/src`;
  let harness: ReturnType<typeof createFlowHarness>;

  beforeEach(() => {
    process.env.WORKSPACE_ROOT = fixtureRepoPath;
    configureImpactFlowRuntime(fixtureRepoPath);
    harness = createFlowHarness(localResearchFlowTools.impactAnalysis);
  });

  afterEach(() => {
    delete process.env.WORKSPACE_ROOT;
    harness.cleanup();
    resetImpactFlowRuntime();
  });

  it('chains localSearchCode -> lspFindReferences -> lspCallHierarchy -> localGetFileContent', async () => {
    const searchResponse = await harness.callTool('localSearchCode', {
      queries: [
        {
          id: 'search_compute_score_definition',
          pattern: 'export function computeScore',
          path: fixtureSourcePath,
          include: ['*.ts'],
          researchGoal: 'Find the computeScore definition',
          reasoning: 'Need the definition line before impact analysis',
        },
      ],
    });

    const searchResult = expectHasResultsData(
      LocalSearchCodeOutputSchema,
      LocalSearchCodeDataSchema,
      searchResponse
    );
    const definitionFile = searchResult.files?.[0];
    const definitionMatch = definitionFile?.matches?.[0];

    const referencesPageOneResponse = await harness.callTool(
      'lspFindReferences',
      {
        queries: [
          {
            id: 'references_page_1',
            uri: definitionFile!.path,
            symbolName: 'computeScore',
            lineHint: definitionMatch!.line,
            includeDeclaration: false,
            referencesPerPage: 1,
            page: 1,
            researchGoal: 'Find the first page of impacted call sites',
            reasoning: 'Start impact analysis from the definition line',
          },
        ],
      }
    );

    const referencesPageOne = expectHasResultsData(
      LspFindReferencesOutputSchema,
      LspFindReferencesDataSchema,
      referencesPageOneResponse
    );

    expect(referencesPageOne.pagination?.hasMore).toBe(true);
    expect(referencesPageOne.hasMultipleFiles).toBe(true);

    const referencesPageTwoResponse = await harness.callTool(
      'lspFindReferences',
      {
        queries: [
          {
            id: 'references_page_2',
            uri: definitionFile!.path,
            symbolName: 'computeScore',
            lineHint: definitionMatch!.line,
            includeDeclaration: false,
            referencesPerPage: 1,
            page: 2,
            researchGoal: 'Find the second page of impacted call sites',
            reasoning: 'Verify pagination preserves the impact-analysis flow',
          },
        ],
      }
    );

    const referencesPageTwo = expectHasResultsData(
      LspFindReferencesOutputSchema,
      LspFindReferencesDataSchema,
      referencesPageTwoResponse
    );

    expect(referencesPageTwo.locations?.[0]?.uri).not.toBe(
      referencesPageOne.locations?.[0]?.uri
    );

    const hierarchyResponse = await harness.callTool('lspCallHierarchy', {
      queries: [
        {
          id: 'incoming_compute_score_calls',
          uri: definitionFile!.path,
          symbolName: 'computeScore',
          lineHint: definitionMatch!.line,
          direction: 'incoming',
          researchGoal: 'Trace functions that would be affected by a change',
          reasoning: 'Use call hierarchy after references to understand impact',
        },
      ],
    });

    const hierarchyResult = expectHasResultsData(
      LspCallHierarchyOutputSchema,
      LspCallHierarchyDataSchema,
      hierarchyResponse
    );

    expect(hierarchyResult.direction).toBe('incoming');
    expect(hierarchyResult.item?.name).toBe('computeScore');
    expect(hierarchyResult.incomingCalls?.length).toBeGreaterThan(0);

    const impactedLocation = referencesPageOne.locations?.[0];
    expect(impactedLocation).toBeDefined();

    const impactedFileResponse = await harness.callTool('localGetFileContent', {
      queries: [
        {
          id: 'inspect_impacted_call_site',
          path: impactedLocation!.uri,
          matchString: 'computeScore',
          researchGoal: 'Inspect an impacted call site',
          reasoning: 'Read one affected file after finding references',
        },
      ],
    });

    const impactedFileResult = expectHasResultsData(
      LocalGetFileContentOutputSchema,
      LocalGetFileContentDataSchema,
      impactedFileResponse
    );

    expect(impactedFileResult.content).toContain('computeScore');
  });

  it('continues impact-analysis flow across reference pagination', async () => {
    const searchResponse = await harness.callTool('localSearchCode', {
      queries: [
        {
          id: 'search_compute_score_definition',
          pattern: 'export function computeScore',
          path: fixtureSourcePath,
          include: ['*.ts'],
          researchGoal: 'Find the computeScore definition',
          reasoning:
            'Need the definition line before output-paginated reference analysis',
        },
      ],
    });

    const searchResult = expectHasResultsData(
      LocalSearchCodeOutputSchema,
      LocalSearchCodeDataSchema,
      searchResponse
    );
    const definitionFile = searchResult.files?.[0];
    const definitionMatch = definitionFile?.matches?.[0];

    const firstReferencesResponse = await harness.callTool(
      'lspFindReferences',
      {
        queries: [
          {
            id: 'references_output_page_1',
            uri: definitionFile!.path,
            symbolName: 'computeScore',
            lineHint: definitionMatch!.line,
            includeDeclaration: false,
            referencesPerPage: 1,
            page: 1,
            researchGoal: 'Read the first output page of impacted call sites',
            reasoning: 'Verify reference pagination on semantic references',
          },
        ],
      }
    );

    const firstReferences = expectHasResultsData(
      LspFindReferencesOutputSchema,
      LspFindReferencesDataSchema,
      firstReferencesResponse
    );

    expect(firstReferences.pagination?.hasMore).toBe(true);
    expect(firstReferences.locations?.length).toBe(1);

    const secondReferencesResponse = await harness.callTool(
      'lspFindReferences',
      {
        queries: [
          {
            id: 'references_output_page_2',
            uri: definitionFile!.path,
            symbolName: 'computeScore',
            lineHint: definitionMatch!.line,
            includeDeclaration: false,
            referencesPerPage: 1,
            page: 2,
            researchGoal: 'Read the next output page of impacted call sites',
            reasoning:
              'Resume the same reference query from pagination metadata',
          },
        ],
      }
    );

    const secondReferences = expectHasResultsData(
      LspFindReferencesOutputSchema,
      LspFindReferencesDataSchema,
      secondReferencesResponse
    );

    expect(
      secondReferences.locations?.[0]?.uri !==
        firstReferences.locations?.[0]?.uri ||
        secondReferences.locations?.[0]?.content !==
          firstReferences.locations?.[0]?.content
    ).toBe(true);

    const hierarchyResponse = await harness.callTool('lspCallHierarchy', {
      queries: [
        {
          id: 'incoming_compute_score_calls_after_output_resume',
          uri: definitionFile!.path,
          symbolName: 'computeScore',
          lineHint: definitionMatch!.line,
          direction: 'incoming',
          researchGoal:
            'Trace impacted callers after resuming paginated references',
          reasoning:
            'Verify reference pagination does not break downstream hierarchy analysis',
        },
      ],
    });

    const hierarchyResult = expectHasResultsData(
      LspCallHierarchyOutputSchema,
      LspCallHierarchyDataSchema,
      hierarchyResponse
    );

    expect(hierarchyResult.incomingCalls?.length).toBeGreaterThan(0);

    const impactedLocation = secondReferences.locations?.[0];
    expect(impactedLocation).toBeDefined();

    const impactedFileResponse = await harness.callTool('localGetFileContent', {
      queries: [
        {
          id: 'inspect_impacted_call_site_after_output_resume',
          path: impactedLocation!.uri,
          matchString: 'computeScore',
          researchGoal:
            'Inspect an impacted call site after resuming pagination',
          reasoning:
            'Verify paginated references still hand off to file inspection',
        },
      ],
    });

    const impactedFileResult = expectHasResultsData(
      LocalGetFileContentOutputSchema,
      LocalGetFileContentDataSchema,
      impactedFileResponse
    );

    expect(impactedFileResult.content).toContain('computeScore');
  });
});

function configureImpactFlowRuntime(repoPath: string): void {
  flowRuntime.checkCommandAvailability.mockReset();
  flowRuntime.checkCommandAvailability.mockImplementation(
    async (command: string) => ({
      available: true,
      command,
      path: `/usr/bin/${command}`,
    })
  );

  flowRuntime.safeExec.mockReset();
  flowRuntime.safeExec.mockImplementation(
    async (command: string, args: string[]) => {
      if (!/rg$/.test(command)) {
        throw new Error(`Unexpected command: ${command}`);
      }

      if (args.includes('export function computeScore')) {
        return {
          success: true,
          code: 0,
          stdout: buildLocalSearchOutput(repoPath),
          stderr: '',
        };
      }

      if (args.includes('\\bcomputeScore\\s*\\(')) {
        return {
          success: true,
          code: 0,
          stdout: buildIncomingCallSearchOutput(repoPath),
          stderr: '',
        };
      }

      throw new Error(`Unexpected safeExec args: ${JSON.stringify(args)}`);
    }
  );

  flowRuntime.spawn.mockReset();
  flowRuntime.spawn.mockImplementation((command: string, args: string[]) => {
    if (!/rg$/.test(command)) {
      throw new Error(`Unexpected spawn command: ${command}`);
    }

    const separatorIndex = args.lastIndexOf('--');
    const symbolName =
      separatorIndex >= 0 && separatorIndex < args.length - 2
        ? args[separatorIndex + 1]
        : undefined;

    if (symbolName !== 'computeScore') {
      throw new Error(`Unexpected spawn args: ${JSON.stringify(args)}`);
    }

    return createMockChildProcess(buildReferenceSearchOutput(repoPath));
  });
}

function resetImpactFlowRuntime(): void {
  flowRuntime.safeExec.mockReset();
  flowRuntime.checkCommandAvailability.mockReset();
  flowRuntime.spawn.mockReset();
}

function createMockChildProcess(stdout: string): EventEmitter & {
  stdout: EventEmitter;
  stderr: EventEmitter;
  kill: ReturnType<typeof vi.fn>;
} {
  const child = new EventEmitter() as EventEmitter & {
    stdout: EventEmitter;
    stderr: EventEmitter;
    kill: ReturnType<typeof vi.fn>;
  };

  child.stdout = new EventEmitter();
  child.stderr = new EventEmitter();
  child.kill = vi.fn();

  queueMicrotask(() => {
    if (stdout) {
      child.stdout.emit('data', Buffer.from(stdout));
    }
    child.emit('close', 0);
  });

  return child;
}

function buildLocalSearchOutput(repoPath: string): string {
  const scorePath = fixtureFilePath(repoPath, 'score.ts');
  const lineText = readFixtureLine(repoPath, 'score.ts', 10);

  return [
    rgBegin(scorePath),
    rgMatch(scorePath, 10, lineText, 'computeScore'),
    rgEnd(scorePath),
    rgSummary(1, 1),
  ].join('\n');
}

function buildIncomingCallSearchOutput(repoPath: string): string {
  return [
    rgMatch(
      fixtureFilePath(repoPath, 'consumer.ts'),
      4,
      readFixtureLine(repoPath, 'consumer.ts', 4),
      'computeScore'
    ),
    rgMatch(
      fixtureFilePath(repoPath, 'secondary.ts'),
      7,
      readFixtureLine(repoPath, 'secondary.ts', 7),
      'computeScore'
    ),
    rgMatch(
      fixtureFilePath(repoPath, 'score.ts'),
      15,
      readFixtureLine(repoPath, 'score.ts', 15),
      'computeScore'
    ),
    rgMatch(
      fixtureFilePath(repoPath, 'score.ts'),
      10,
      readFixtureLine(repoPath, 'score.ts', 10),
      'computeScore'
    ),
  ].join('\n');
}

function buildReferenceSearchOutput(repoPath: string): string {
  return [
    rgMatch(
      fixtureFilePath(repoPath, 'consumer.ts'),
      4,
      readFixtureLine(repoPath, 'consumer.ts', 4),
      'computeScore'
    ),
    rgMatch(
      fixtureFilePath(repoPath, 'secondary.ts'),
      7,
      readFixtureLine(repoPath, 'secondary.ts', 7),
      'computeScore'
    ),
    rgMatch(
      fixtureFilePath(repoPath, 'score.ts'),
      15,
      readFixtureLine(repoPath, 'score.ts', 15),
      'computeScore'
    ),
    rgMatch(
      fixtureFilePath(repoPath, 'score.ts'),
      10,
      readFixtureLine(repoPath, 'score.ts', 10),
      'computeScore'
    ),
  ].join('\n');
}

function fixtureFilePath(repoPath: string, fileName: string): string {
  return path.join(repoPath, 'src', fileName);
}

function readFixtureLine(
  repoPath: string,
  fileName: string,
  lineNumber: number
): string {
  const fileContent = readFileSync(fixtureFilePath(repoPath, fileName), 'utf8');
  return fileContent.split(/\r?\n/)[lineNumber - 1] ?? '';
}

function rgBegin(filePath: string): string {
  return JSON.stringify({
    type: 'begin',
    data: { path: { text: filePath } },
  });
}

function rgEnd(filePath: string): string {
  return JSON.stringify({
    type: 'end',
    data: {
      path: { text: filePath },
      stats: {
        elapsed: { human: '0.001s' },
        searches: 1,
        searches_with_match: 1,
      },
    },
  });
}

function rgSummary(searches: number, matches: number): string {
  return JSON.stringify({
    type: 'summary',
    data: {
      elapsed_total: { human: '0.001s' },
      stats: {
        elapsed: { human: '0.001s' },
        searches,
        searches_with_match: searches,
        bytes_searched: 512,
        bytes_printed: 256,
        matched_lines: matches,
        matches,
      },
    },
  });
}

function rgMatch(
  filePath: string,
  lineNumber: number,
  lineText: string,
  symbolName: string
): string {
  const start = Math.max(0, lineText.indexOf(symbolName));

  return JSON.stringify({
    type: 'match',
    data: {
      path: { text: filePath },
      lines: { text: `${lineText}\n` },
      line_number: lineNumber,
      absolute_offset: lineNumber * 100,
      submatches: [
        {
          match: { text: symbolName },
          start,
          end: start + symbolName.length,
        },
      ],
    },
  });
}
