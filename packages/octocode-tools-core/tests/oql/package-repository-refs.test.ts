import { beforeEach, describe, expect, it, vi } from 'vitest';

const { runDirect } = vi.hoisted(() => ({ runDirect: vi.fn() }));

vi.mock('../../src/oql/adapters/runner.js', () => ({ runDirect }));

import { runOqlSearch } from '../../src/oql/run.js';

describe('OQL package repository references', () => {
  beforeEach(() => {
    runDirect.mockReset();
  });

  it('keeps compact npm repository ids tied to a shared repository table', async () => {
    runDirect.mockResolvedValue({
      content: [{ type: 'text', text: 'stubbed npmSearch response' }],
      isError: false,
      structuredContent: {
        results: [
          {
            id: 'npmSearch-1',
            status: 'success',
            data: {
              packages: [
                {
                  name: 'octocode-cli',
                  version: '1.0.0',
                  repositoryId: 'r1',
                },
                {
                  name: 'octocode',
                  version: '2.0.0',
                  repositoryId: 'r1',
                },
              ],
              repositories: {
                r1: {
                  repository: 'https://github.com/bgauryy/octocode-mcp',
                  owner: 'bgauryy',
                  repo: 'octocode-mcp',
                  repositoryDirectory: 'packages/octocode',
                  next: {
                    viewRepoStructure: {
                      tool: 'ghViewRepoStructure',
                      query: {
                        owner: 'bgauryy',
                        repo: 'octocode-mcp',
                        path: 'packages/octocode',
                      },
                    },
                  },
                },
              },
              pagination: { hasMore: false, currentPage: 1, totalPages: 1 },
            },
          },
        ],
      },
    });

    const envelope = await runOqlSearch({
      schema: 'oql',
      target: 'packages',
      params: { packageName: 'code research' },
      limit: 2,
    });

    expect(envelope.results).toHaveLength(2);
    expect(envelope.shared?.repositories).toMatchObject({
      r1: {
        repository: 'https://github.com/bgauryy/octocode-mcp',
        owner: 'bgauryy',
        repo: 'octocode-mcp',
        repositoryDirectory: 'packages/octocode',
      },
    });
    expect(
      (envelope.shared?.repositories as Record<string, Record<string, unknown>>)
        .r1
    ).not.toHaveProperty('next');
    for (const row of envelope.results) {
      expect(row.kind).toBe('record');
      if (row.kind !== 'record') continue;
      expect(row.recordType).toBe('package');
      expect(row.data).toMatchObject({
        repositoryId: 'r1',
      });
      expect(row.data).not.toHaveProperty('repository');
      expect(row.data).not.toHaveProperty('owner');
      expect(row.data).not.toHaveProperty('repo');
      expect(row.data).not.toHaveProperty('next');
    }
  });

  it('hoists repeated row continuation prose into envelope nextHints', async () => {
    const envelope = await runOqlSearch({
      schema: 'oql',
      target: 'code',
      from: { kind: 'local', path: 'src/oql' },
      where: { kind: 'text', value: 'runOqlSearch' },
      view: 'discovery',
      limit: 2,
    });

    expect(envelope.nextHints?.['next.fetch']).toMatchObject({
      why: 'Read the exact content at this hit.',
      confidence: 'exact',
    });
    expect(envelope.results.length).toBeGreaterThan(0);
    const firstRow = envelope.results[0] as {
      next?: Record<string, Record<string, unknown>>;
    };
    expect(firstRow.next?.['next.fetch']?.query).toMatchObject({
      target: 'content',
    });
    expect(firstRow.next?.['next.fetch']).not.toHaveProperty('why');
    expect(firstRow.next?.['next.fetch']).not.toHaveProperty('confidence');
  });
});
