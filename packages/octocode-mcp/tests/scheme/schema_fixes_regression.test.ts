import { describe, expect, it } from 'vitest';
import {
  BulkCloneRepoLocalSchema,
  GitHubCloneRepoOutputLocalSchema,
  GitHubCodeSearchOutputLocalSchema,
  GitHubFetchContentOutputLocalSchema,
} from '../../src/scheme/remoteSchemaOverlay.js';
import {
  BulkRipgrepQuerySchema,
  FetchContentQuerySchema,
  FindFilesQuerySchema,
  RipgrepQuerySchema,
  ViewStructureQuerySchema,
} from '../../src/scheme/localSchemaOverlay.js';
import {
  BulkLSPCallHierarchyQuerySchema,
  BulkLSPFindReferencesQuerySchema,
  BulkLSPGotoDefinitionQuerySchema,
  LSPCallHierarchyQuerySchema,
  LSPFindReferencesQuerySchema,
  LSPGotoDefinitionQuerySchema,
} from '../../src/scheme/lspSchemaOverlay.js';

describe('schema fixes regression', () => {
  it('rejects non-object bulk query entries instead of dropping them', () => {
    const result = BulkRipgrepQuerySchema.safeParse({
      queries: [
        {
          id: 'valid_query',
          pattern: 'export',
          path: '/tmp',
        },
        'not a query',
      ],
    });

    expect(result.success).toBe(false);
  });

  it('accepts mainResearchGoal consistently across local and LSP overlays', () => {
    const localSchemas = [
      {
        schema: RipgrepQuerySchema,
        payload: {
          id: 'ripgrep_main_goal',
          mainResearchGoal: 'Understand local search',
          pattern: 'export',
          path: '/tmp',
        },
      },
      {
        schema: FindFilesQuerySchema,
        payload: {
          id: 'find_main_goal',
          mainResearchGoal: 'Find files',
          path: '/tmp',
        },
      },
      {
        schema: ViewStructureQuerySchema,
        payload: {
          id: 'view_main_goal',
          mainResearchGoal: 'Inspect structure',
          path: '/tmp',
        },
      },
      {
        schema: FetchContentQuerySchema,
        payload: {
          id: 'fetch_main_goal',
          mainResearchGoal: 'Read content',
          path: '/tmp/file.ts',
        },
      },
    ];

    for (const { schema, payload } of localSchemas) {
      expect(schema.safeParse(payload).success).toBe(true);
    }

    const lspSchemas = [
      {
        schema: LSPGotoDefinitionQuerySchema,
        payload: {
          id: 'goto_main_goal',
          mainResearchGoal: 'Resolve symbols',
          uri: '/tmp/file.ts',
          symbolName: 'foo',
          lineHint: 1,
        },
      },
      {
        schema: LSPFindReferencesQuerySchema,
        payload: {
          id: 'refs_main_goal',
          mainResearchGoal: 'Resolve symbols',
          uri: '/tmp/file.ts',
          symbolName: 'foo',
          lineHint: 1,
        },
      },
      {
        schema: LSPCallHierarchyQuerySchema,
        payload: {
          id: 'calls_main_goal',
          mainResearchGoal: 'Resolve symbols',
          uri: '/tmp/file.ts',
          symbolName: 'foo',
          lineHint: 1,
          direction: 'incoming',
        },
      },
    ];

    for (const { schema, payload } of lspSchemas) {
      expect(schema.safeParse(payload).success).toBe(true);
    }

    expect(
      BulkLSPGotoDefinitionQuerySchema.safeParse({
        queries: [lspSchemas[0].payload],
      }).success
    ).toBe(true);
    expect(
      BulkLSPFindReferencesQuerySchema.safeParse({
        queries: [lspSchemas[1].payload],
      }).success
    ).toBe(true);
    expect(
      BulkLSPCallHierarchyQuerySchema.safeParse({
        queries: [lspSchemas[2].payload],
      }).success
    ).toBe(true);
  });

  it('custom remote outputs accept evidence envelope metadata', () => {
    const evidence = {
      kind: 'code' as const,
      answerReady: true,
      confidence: 'high' as const,
      complete: true,
    };

    expect(
      GitHubCodeSearchOutputLocalSchema.safeParse({
        results: [],
        evidence,
      }).success
    ).toBe(true);

    expect(
      GitHubFetchContentOutputLocalSchema.safeParse({
        results: [],
        evidence: { ...evidence, kind: 'content' as const },
      }).success
    ).toBe(true);
  });

  it('githubCloneRepo input/output schemas expose the common bulk and envelope fields', () => {
    const parsedInput = BulkCloneRepoLocalSchema.parse({
      queries: [
        {
          id: 'clone',
          mainResearchGoal: 'Clone repo',
          researchGoal: 'Get a local checkout',
          reasoning: 'Local tools need a repository path',
          owner: 'octocat',
          repo: 'hello-world',
        },
      ],
    });
    expect(parsedInput.format).toBe('tsv');

    const output = GitHubCloneRepoOutputLocalSchema.safeParse({
      results: [
        {
          id: 'clone',
          status: 'hasResults',
          data: { localPath: '/tmp/repo' },
        },
      ],
      format: 'tsv',
      columns: ['localPath'],
      rows: '/tmp/repo',
      hints: ['Use local tools next'],
      evidence: {
        kind: 'content',
        answerReady: true,
        confidence: 'high',
        complete: true,
      },
    });

    expect(output.success).toBe(true);
  });
});
