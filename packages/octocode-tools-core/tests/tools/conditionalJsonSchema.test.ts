import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import { FileContentBulkQueryLocalSchema } from '../../src/tools/github_fetch_content/scheme.js';
import {
  SearchCommitsBulkLocalSchema,
  SearchIssuesBulkLocalSchema,
  SearchPullRequestsBulkLocalSchema,
} from '../../src/tools/github_search_pull_requests/splitSchemes.js';
import { LocalFetchContentBulkQuerySchema } from '../../src/tools/local_fetch_content/scheme.js';
import { BulkLspGetSemanticsQuerySchema } from '../../src/tools/lsp/semantic_content/scheme.js';
import { NpmSearchBulkQueryLocalSchema } from '../../src/tools/package_search/scheme.js';

function acceptsGeneratedSchema(
  schema: z.ZodTypeAny,
  query: Record<string, unknown>
): boolean {
  const jsonSchema = z.toJSONSchema(schema, { io: 'input' });
  const generatedValidator = z.fromJSONSchema(jsonSchema);
  return generatedValidator.safeParse({ queries: [query] }).success;
}

function expectAccepted(
  schema: z.ZodTypeAny,
  ...queries: Array<Record<string, unknown>>
): void {
  for (const query of queries) {
    expect(query, JSON.stringify(query)).toSatisfy(candidate =>
      acceptsGeneratedSchema(schema, candidate)
    );
  }
}

function expectRejected(
  schema: z.ZodTypeAny,
  ...queries: Array<Record<string, unknown>>
): void {
  for (const query of queries) {
    expect(query, JSON.stringify(query)).not.toSatisfy(candidate =>
      acceptsGeneratedSchema(schema, candidate)
    );
  }
}

describe('generated conditional input schemas', () => {
  it('encodes npm packageName xor keywords', () => {
    expectAccepted(
      NpmSearchBulkQueryLocalSchema,
      { packageName: 'zod' },
      { keywords: ['schema', 'validation'] }
    );
    expectRejected(
      NpmSearchBulkQueryLocalSchema,
      {},
      { packageName: 'zod', keywords: ['schema'] }
    );
  });

  it.each([
    ['local', LocalFetchContentBulkQuerySchema, { path: '/repo/src/a.ts' }],
    [
      'github',
      FileContentBulkQueryLocalSchema,
      { owner: 'octo', repo: 'repo', path: 'src/a.ts' },
    ],
  ])('encodes %s content selector modes', (_name, schema, base) => {
    expectAccepted(
      schema,
      base,
      { ...base, fullContent: true },
      { ...base, matchString: 'export function' },
      { ...base, startLine: 2, endLine: 8 }
    );
    expectRejected(
      schema,
      { ...base, startLine: 2 },
      { ...base, endLine: 8 },
      { ...base, matchStringIsRegex: true },
      { ...base, fullContent: true, matchString: 'export function' },
      { ...base, matchString: 'export function', startLine: 2, endLine: 8 }
    );
  });

  it('keeps GitHub directory materialization separate from file extraction', () => {
    const base = {
      owner: 'octo',
      repo: 'repo',
      path: 'src',
      type: 'directory',
    };
    expectAccepted(FileContentBulkQueryLocalSchema, base);
    expectRejected(
      FileContentBulkQueryLocalSchema,
      { ...base, fullContent: true },
      { ...base, matchString: 'needle' },
      { ...base, startLine: 1, endLine: 2 }
    );
  });

  it('encodes commit history and compare modes as separate executor routes', () => {
    const base = { owner: 'octo', repo: 'repo' };
    expectAccepted(
      SearchCommitsBulkLocalSchema,
      { ...base, branch: 'main', since: '30d' },
      { ...base, base: 'main', head: 'feature', path: 'src/' }
    );
    expectRejected(
      SearchCommitsBulkLocalSchema,
      { ...base, base: 'main' },
      { ...base, head: 'feature' },
      { ...base, base: 'main', head: 'feature', branch: 'main' },
      { ...base, base: 'main', head: 'feature', page: 2 }
    );
  });

  it('encodes PR list/detail modes and patch selection', () => {
    const base = { owner: 'octo', repo: 'repo' };
    expectAccepted(
      SearchPullRequestsBulkLocalSchema,
      { ...base, keywords: ['schema'], page: 2 },
      { ...base, prNumber: 7, content: { patches: { mode: 'all' } } },
      {
        ...base,
        prNumber: 7,
        content: { patches: { mode: 'selected', files: ['src/a.ts'] } },
      },
      {
        ...base,
        prNumber: 7,
        content: {
          patches: {
            mode: 'selected',
            ranges: [{ file: 'src/a.ts', additions: [3] }],
          },
        },
      }
    );
    expectRejected(
      SearchPullRequestsBulkLocalSchema,
      { ...base, content: { body: true } },
      { ...base, prNumber: 7, keywords: ['ignored'] },
      {
        ...base,
        prNumber: 7,
        content: { patches: { mode: 'selected' } },
      },
      {
        ...base,
        prNumber: 7,
        content: { patches: { mode: 'all', files: ['src/a.ts'] } },
      }
    );
  });

  it('encodes issue list and detail modes', () => {
    const base = { owner: 'octo', repo: 'repo' };
    expectAccepted(
      SearchIssuesBulkLocalSchema,
      { ...base, keywords: ['schema'], page: 2 },
      { ...base, issueNumber: 7, content: { body: true } }
    );
    expectRejected(
      SearchIssuesBulkLocalSchema,
      { ...base, content: { body: true } },
      { ...base, issueNumber: 7, state: 'open' }
    );
  });

  it('encodes anchored, document, and workspace LSP requirements', () => {
    expectAccepted(
      BulkLspGetSemanticsQuerySchema,
      { uri: '/repo/src/a.ts', symbolName: 'run', lineHint: 4 },
      { uri: '/repo/src/a.ts', type: 'documentSymbols' },
      { uri: '/repo/src/a.ts', type: 'diagnostic' },
      { type: 'workspaceSymbol', symbolName: 'Schema' },
      {
        type: 'workspaceSymbol',
        symbolName: 'Schema',
        workspaceRoot: '/repo',
      }
    );
    expectRejected(
      BulkLspGetSemanticsQuerySchema,
      {},
      { uri: '/repo/src/a.ts' },
      { type: 'documentSymbols' },
      { type: 'workspaceSymbol' },
      { type: 'definition', uri: '/repo/src/a.ts', symbolName: 'run' }
    );
  });
});
