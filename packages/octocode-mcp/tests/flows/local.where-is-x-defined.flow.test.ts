import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  LocalGetFileContentDataSchema,
  LocalGetFileContentOutputSchema,
  LocalSearchCodeDataSchema,
  LocalSearchCodeOutputSchema,
  LspGotoDefinitionDataSchema,
  LspGotoDefinitionOutputSchema,
} from '@octocodeai/octocode-core';
import { FLOW_CATALOG } from './catalog.js';
import {
  createFlowHarness,
  getFlowFixturePath,
  localResearchFlowTools,
} from './harness.js';
import { expectHasResultsData, getSingleResult } from './assertions.js';
import {
  configureLocalResearchFlowRuntime,
  mockSafeExec,
  mockSpawn,
  resetLocalResearchFlowRuntime,
} from './runtime.mocks.js';

vi.mock('../../src/utils/exec/safe.js', async () => {
  const actual = await vi.importActual<
    typeof import('../../src/utils/exec/safe.js')
  >('../../src/utils/exec/safe.js');
  const runtime = await import('./runtime.mocks.js');

  return {
    ...actual,
    safeExec: runtime.mockSafeExec,
  };
});

vi.mock('../../src/utils/exec/commandAvailability.js', async () => {
  const actual = await vi.importActual<
    typeof import('../../src/utils/exec/commandAvailability.js')
  >('../../src/utils/exec/commandAvailability.js');
  const runtime = await import('./runtime.mocks.js');

  return {
    ...actual,
    checkCommandAvailability: runtime.mockCheckCommandAvailability,
  };
});

vi.mock('child_process', async () => {
  const runtime = await import('./runtime.mocks.js');
  return {
    spawn: runtime.mockSpawn,
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

describe(FLOW_CATALOG.localWhereIsXDefined.id, () => {
  const fixtureRepoPath = getFlowFixturePath('mini-ts-repo');
  const fixtureSourcePath = `${fixtureRepoPath}/src`;
  let harness: ReturnType<typeof createFlowHarness>;

  beforeEach(() => {
    process.env.WORKSPACE_ROOT = fixtureRepoPath;
    configureLocalResearchFlowRuntime(fixtureRepoPath);
    harness = createFlowHarness(localResearchFlowTools.whereIsXDefined);
  });

  afterEach(() => {
    delete process.env.WORKSPACE_ROOT;
    harness.cleanup();
    resetLocalResearchFlowRuntime();
  });

  it('chains localSearchCode -> lspGotoDefinition -> localGetFileContent via real handoff fields', async () => {
    const searchResponse = await harness.callTool('localSearchCode', {
      queries: [
        {
          id: 'search_compute_score_definition',
          pattern: 'export function computeScore',
          path: fixtureSourcePath,
          include: ['*.ts'],
          researchGoal: 'Find the computeScore definition',
          reasoning: 'Need lineHint before goto definition',
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

    expect(definitionFile?.path.endsWith('/src/score.ts')).toBe(true);
    expect(definitionMatch?.line).toBeTypeOf('number');

    const gotoResponse = await harness.callTool('lspGotoDefinition', {
      queries: [
        {
          id: 'goto_compute_score_definition',
          uri: definitionFile!.path,
          symbolName: 'computeScore',
          lineHint: definitionMatch!.line,
          researchGoal: 'Resolve the computeScore definition',
          reasoning: 'Use the lineHint from localSearchCode to jump to code',
        },
      ],
    });

    const gotoResult = expectHasResultsData(
      LspGotoDefinitionOutputSchema,
      LspGotoDefinitionDataSchema,
      gotoResponse
    );
    const location = gotoResult.locations?.[0];

    expect(location?.uri).toBe(definitionFile?.path);
    expect(location?.content).toContain('export function computeScore');

    const fileContentResponse = await harness.callTool('localGetFileContent', {
      queries: [
        {
          id: 'fetch_compute_score_definition',
          path: location!.uri,
          matchString: 'export function computeScore',
          researchGoal: 'Read the computeScore implementation',
          reasoning: 'Need the function body after locating the definition',
        },
      ],
    });

    const fileContentResult = expectHasResultsData(
      LocalGetFileContentOutputSchema,
      LocalGetFileContentDataSchema,
      fileContentResponse
    );

    expect(fileContentResult.content).toContain('normalizeScore');
    expect(fileContentResult.matchRanges?.length).toBeGreaterThan(0);
  });

  it('returns an empty result with guidance when the symbol handoff is stale', async () => {
    const gotoResponse = await harness.callTool('lspGotoDefinition', {
      queries: [
        {
          id: 'goto_missing_symbol',
          uri: `${fixtureSourcePath}/score.ts`,
          symbolName: 'missingScore',
          lineHint: 1,
          researchGoal: 'Resolve a stale symbol reference',
          reasoning: 'Verify the flow fails cleanly when the symbol changed',
        },
      ],
    });

    const gotoResult = getSingleResult(
      LspGotoDefinitionOutputSchema,
      gotoResponse
    );

    expect(gotoResult.status).toBe('empty');
    expect(gotoResult.data).toMatchObject({
      errorType: 'symbol_not_found',
    });
  });

  it('blocks path traversal before executing shell commands', async () => {
    const invalidSearchResponse = await harness.callTool('localSearchCode', {
      queries: [
        {
          id: 'search_outside_workspace',
          pattern: 'computeScore',
          path: '/etc',
          include: ['*.ts'],
          researchGoal: 'Attempt an invalid traversal search',
          reasoning: 'Security flow coverage: invalid path must fail closed',
        },
      ],
    });

    const invalidResult = getSingleResult(
      LocalSearchCodeOutputSchema,
      invalidSearchResponse
    );

    expect(invalidResult.status).toBe('error');
    expect(mockSafeExec).not.toHaveBeenCalled();
    expect(mockSpawn).not.toHaveBeenCalled();
  });

  it('runs the where-is-x-defined chain without unnecessary shell spawns', async () => {
    const searchResponse = await harness.callTool('localSearchCode', {
      queries: [
        {
          id: 'search_compute_score_efficiency',
          pattern: 'export function computeScore',
          path: fixtureSourcePath,
          include: ['*.ts'],
          researchGoal: 'Find the computeScore definition',
          reasoning: 'Efficiency flow coverage before downstream handoff',
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

    const gotoResponse = await harness.callTool('lspGotoDefinition', {
      queries: [
        {
          id: 'goto_compute_score_efficiency',
          uri: definitionFile!.path,
          symbolName: 'computeScore',
          lineHint: definitionMatch!.line,
          researchGoal: 'Resolve definition for efficiency contract',
          reasoning: 'Only localSearchCode should require rg command execution',
        },
      ],
    });

    const gotoResult = expectHasResultsData(
      LspGotoDefinitionOutputSchema,
      LspGotoDefinitionDataSchema,
      gotoResponse
    );

    await harness.callTool('localGetFileContent', {
      queries: [
        {
          id: 'fetch_compute_score_efficiency',
          path: gotoResult.locations![0]!.uri,
          matchString: 'export function computeScore',
          researchGoal: 'Inspect resolved definition',
          reasoning: 'Complete the flow while validating command efficiency',
        },
      ],
    });

    expect(mockSafeExec).toHaveBeenCalledTimes(1);
    expect(mockSpawn).not.toHaveBeenCalled();
  });

  it('continues local where-is-x-defined across bulk response pagination', async () => {
    const pagedSearchResponse = await harness.callTool('localSearchCode', {
      queries: [
        {
          id: 'search_page_one',
          pattern: 'export function computeScore',
          path: fixtureSourcePath,
          include: ['*.ts'],
          researchGoal: 'Find the computeScore definition (page 1)',
          reasoning: 'First query in a bulk response pagination flow',
        },
        {
          id: 'search_page_two',
          pattern: 'export function computeScore',
          path: fixtureSourcePath,
          include: ['*.ts'],
          researchGoal: 'Find the computeScore definition (page 2)',
          reasoning: 'Second query in a bulk response pagination flow',
        },
      ],
      responseCharLength: 200,
    });

    const pagedSearchOutput = LocalSearchCodeOutputSchema.parse(
      pagedSearchResponse.structuredContent
    );

    expect(pagedSearchOutput.responsePagination?.hasMore).toBe(true);
    expect(pagedSearchOutput.results).toHaveLength(1);
    expect(pagedSearchOutput.results[0]?.id).toBe('search_page_one');

    const resumedSearchResponse = await harness.callTool('localSearchCode', {
      queries: [
        {
          id: 'search_page_one',
          pattern: 'export function computeScore',
          path: fixtureSourcePath,
          include: ['*.ts'],
          researchGoal: 'Find the computeScore definition (page 1)',
          reasoning: 'First query in a bulk response pagination flow',
        },
        {
          id: 'search_page_two',
          pattern: 'export function computeScore',
          path: fixtureSourcePath,
          include: ['*.ts'],
          researchGoal: 'Find the computeScore definition (page 2)',
          reasoning: 'Second query in a bulk response pagination flow',
        },
      ],
      responseCharLength: 200,
      responseCharOffset:
        (pagedSearchOutput.responsePagination?.charOffset ?? 0) +
        (pagedSearchOutput.responsePagination?.charLength ?? 0),
    });

    const resumedSearchResult = expectHasResultsData(
      LocalSearchCodeOutputSchema,
      LocalSearchCodeDataSchema,
      resumedSearchResponse
    );
    const definitionFile = resumedSearchResult.files?.[0];
    const definitionMatch = definitionFile?.matches?.[0];

    expect(definitionFile?.path.endsWith('/src/score.ts')).toBe(true);

    const gotoResponse = await harness.callTool('lspGotoDefinition', {
      queries: [
        {
          id: 'goto_compute_score_definition_after_bulk_resume',
          uri: definitionFile!.path,
          symbolName: 'computeScore',
          lineHint: definitionMatch!.line,
          researchGoal: 'Resolve the computeScore definition after bulk resume',
          reasoning: 'Use the result returned on the second bulk response page',
        },
      ],
    });

    const gotoResult = expectHasResultsData(
      LspGotoDefinitionOutputSchema,
      LspGotoDefinitionDataSchema,
      gotoResponse
    );
    const location = gotoResult.locations?.[0];

    const fileContentResponse = await harness.callTool('localGetFileContent', {
      queries: [
        {
          id: 'fetch_compute_score_definition_after_bulk_resume',
          path: location!.uri,
          matchString: 'export function computeScore',
          researchGoal: 'Read the resumed computeScore implementation',
          reasoning:
            'Verify bulk response pagination preserves the handoff chain',
        },
      ],
    });

    const fileContentResult = expectHasResultsData(
      LocalGetFileContentOutputSchema,
      LocalGetFileContentDataSchema,
      fileContentResponse
    );

    expect(fileContentResult.content).toContain('normalizeScore');
  });
});
