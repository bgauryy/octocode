import { EventEmitter } from 'events';
import { readFileSync } from 'fs';
import path from 'path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { processCallHierarchy } from '../../src/tools/lsp_call_hierarchy/callHierarchy.js';
import { fetchContent } from '../../src/tools/local_fetch_content/fetchContent.js';
import { searchContentRipgrep } from '../../src/tools/local_ripgrep/searchContentRipgrep.js';
import { findReferences } from '../../src/tools/lsp_find_references/lsp_find_references.js';
import { FLOW_CATALOG } from './catalog.js';
import { getFlowFixturePath } from './harness.js';

const flowRuntime = vi.hoisted(() => ({
  safeExec: vi.fn(),
  checkCommandAvailability: vi.fn(),
  spawn: vi.fn(),
}));

vi.mock('../../src/utils/exec/safe.js', () => ({
  safeExec: flowRuntime.safeExec,
}));

vi.mock('../../src/utils/exec/commandAvailability.js', () => ({
  checkCommandAvailability: flowRuntime.checkCommandAvailability,
  getMissingCommandError: vi.fn().mockReturnValue('Command not available'),
}));

vi.mock('child_process', () => ({
  spawn: flowRuntime.spawn,
}));

vi.mock('octocode-security-utils/commandValidator', () => ({
  validateCommand: vi.fn().mockReturnValue({ isValid: true }),
}));

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

describe(`${FLOW_CATALOG.localImpactAnalysis.id}.execution`, () => {
  const fixtureRepoPath = getFlowFixturePath('mini-ts-repo');
  const fixtureSourcePath = `${fixtureRepoPath}/src`;

  beforeEach(() => {
    process.env.WORKSPACE_ROOT = fixtureRepoPath;
    configureExecutionFlowRuntime(fixtureRepoPath);
  });

  afterEach(() => {
    delete process.env.WORKSPACE_ROOT;
    resetExecutionFlowRuntime();
  });

  it('chains the actual execution helpers for impact analysis', async () => {
    const searchResult = await searchContentRipgrep({
      id: 'search_compute_score_definition',
      pattern: 'export function computeScore',
      path: fixtureSourcePath,
      include: ['*.ts'],
      smartCase: true,
      matchContentLength: 200,
      filesPerPage: 10,
      filePageNumber: 1,
      matchesPerPage: 10,
      binaryFiles: 'without-match',
      sort: 'path',
      includeStats: true,
      showFileLastModified: false,
      researchGoal: 'Find the computeScore definition',
      reasoning: 'Need the definition line before impact analysis',
    });

    expect(searchResult.status).toBe('hasResults');
    if (searchResult.status !== 'hasResults') {
      throw new Error(JSON.stringify(searchResult, null, 2));
    }

    const definitionFile = searchResult.files?.[0];
    const definitionMatch = definitionFile?.matches?.[0];

    expect(definitionFile?.path.endsWith('/src/score.ts')).toBe(true);
    expect(definitionMatch?.line).toBe(10);

    const referencesPageOne = await findReferences({
      id: 'references_page_1',
      uri: definitionFile!.path,
      symbolName: 'computeScore',
      lineHint: definitionMatch!.line,
      orderHint: 0,
      includeDeclaration: false,
      contextLines: 2,
      referencesPerPage: 1,
      page: 1,
      researchGoal: 'Find the first impacted call site',
      reasoning: 'Start impact analysis from the definition line',
    });

    expect(referencesPageOne.status).toBe('hasResults');
    if (referencesPageOne.status !== 'hasResults') {
      throw new Error(JSON.stringify(referencesPageOne, null, 2));
    }

    expect(referencesPageOne.pagination?.hasMore).toBe(true);
    expect(referencesPageOne.hasMultipleFiles).toBe(true);

    const referencesPageTwo = await findReferences({
      id: 'references_page_2',
      uri: definitionFile!.path,
      symbolName: 'computeScore',
      lineHint: definitionMatch!.line,
      orderHint: 0,
      includeDeclaration: false,
      contextLines: 2,
      referencesPerPage: 1,
      page: 2,
      researchGoal: 'Find the second impacted call site',
      reasoning: 'Verify pagination preserves the impact-analysis flow',
    });

    expect(referencesPageTwo.status).toBe('hasResults');
    if (referencesPageTwo.status !== 'hasResults') {
      throw new Error(JSON.stringify(referencesPageTwo, null, 2));
    }

    expect(referencesPageTwo.locations?.[0]?.uri).not.toBe(
      referencesPageOne.locations?.[0]?.uri
    );

    const hierarchyResult = await processCallHierarchy({
      id: 'incoming_compute_score_calls',
      uri: definitionFile!.path,
      symbolName: 'computeScore',
      lineHint: definitionMatch!.line,
      orderHint: 0,
      direction: 'incoming',
      depth: 1,
      contextLines: 2,
      callsPerPage: 15,
      page: 1,
      researchGoal: 'Trace impacted callers',
      reasoning: 'Use call hierarchy after references to understand impact',
    });

    expect(hierarchyResult.status).toBe('hasResults');
    if (hierarchyResult.status !== 'hasResults') {
      throw new Error(JSON.stringify(hierarchyResult, null, 2));
    }

    expect(hierarchyResult.direction).toBe('incoming');
    expect(hierarchyResult.item?.name).toBe('computeScore');
    expect(hierarchyResult.incomingCalls?.length).toBeGreaterThan(0);

    const impactedLocation = referencesPageOne.locations?.[0];
    expect(impactedLocation).toBeDefined();
    const impactedPath = path.isAbsolute(impactedLocation!.uri)
      ? impactedLocation!.uri
      : path.join(fixtureRepoPath, impactedLocation!.uri);

    const impactedFile = await fetchContent({
      id: 'inspect_impacted_call_site',
      path: impactedPath,
      matchString: 'computeScore',
      fullContent: false,
      matchStringContextLines: 5,
      matchStringIsRegex: false,
      matchStringCaseSensitive: false,
      researchGoal: 'Read one impacted file',
      reasoning: 'Inspect the first impacted call site',
    } as Parameters<typeof fetchContent>[0]);

    expect(impactedFile.status).toBe('hasResults');
    if (impactedFile.status !== 'hasResults') {
      throw new Error(JSON.stringify(impactedFile, null, 2));
    }

    expect(impactedFile.content).toContain('computeScore');
  });

  it('blocks traversal paths before ripgrep execution', async () => {
    const result = await searchContentRipgrep({
      id: 'search_outside_workspace',
      pattern: 'computeScore',
      path: '/etc',
      include: ['*.ts'],
      smartCase: true,
      matchContentLength: 120,
      filesPerPage: 10,
      filePageNumber: 1,
      matchesPerPage: 10,
      binaryFiles: 'without-match',
      sort: 'path',
      includeStats: false,
      showFileLastModified: false,
      researchGoal: 'Attempt invalid traversal for security coverage',
      reasoning: 'Flow must fail closed before command execution',
    });

    expect(result.status).toBe('error');
    expect(flowRuntime.safeExec).not.toHaveBeenCalled();
    expect(flowRuntime.spawn).not.toHaveBeenCalled();
  });

  it('returns empty out-of-range reference pages with pagination guidance', async () => {
    const outOfRange = await findReferences({
      id: 'references_out_of_range',
      uri: `${fixtureSourcePath}/score.ts`,
      symbolName: 'computeScore',
      lineHint: 10,
      orderHint: 0,
      includeDeclaration: false,
      contextLines: 2,
      referencesPerPage: 1,
      page: 99,
      researchGoal: 'Request an out-of-range page',
      reasoning:
        'Efficiency coverage: avoid downstream work when page is invalid',
    });

    expect(outOfRange.status).toBe('empty');

    if (outOfRange.status === 'empty') {
      expect(outOfRange.pagination?.currentPage).toBe(99);
      expect(outOfRange.pagination?.totalPages).toBeGreaterThan(0);
      expect(
        outOfRange.hints?.some(hint => hint.includes('outside available range'))
      ).toBe(true);
    }

    expect(flowRuntime.spawn).toHaveBeenCalledTimes(1);
  });
});

function configureExecutionFlowRuntime(repoPath: string): void {
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

function resetExecutionFlowRuntime(): void {
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
