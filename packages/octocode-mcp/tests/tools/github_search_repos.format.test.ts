import { describe, it, expect } from 'vitest';
import { formatRepoLine } from '../../../octocode-tools-core/src/tools/github_search_repos/execution.js';

const base = {
  owner: 'punkpeye',
  repo: 'awesome-mcp-servers',
  defaultBranch: 'main',
  stars: 88688,
  description: 'A collection of MCP servers.',
  url: 'https://github.com/punkpeye/awesome-mcp-servers',
  createdAt: '2024-11-30T04:49:10Z',
  updatedAt: '2026-06-08T11:25:01Z',
  pushedAt: '2026-06-07T21:38:07Z',
  visibility: 'public',
  topics: [],
  forksCount: 6829,
  openIssuesCount: 496,
  language: 'TypeScript',
} as unknown as Parameters<typeof formatRepoLine>[0];

describe('formatRepoLine — compact one-liner', () => {
  it('packs path, stars, forks, issues, language, dates, description', () => {
    const line = formatRepoLine(base);
    expect(line).toContain('punkpeye/awesome-mcp-servers');
    expect(line).toContain('88688 stars');
    expect(line).toContain('6829 forks');
    expect(line).toContain('496 issues');
    expect(line).toContain('TypeScript');
    expect(line).toContain('2026-06-07');
    expect(line).toContain('A collection of MCP servers.');
  });

  it('omits redundant/default fields (url, updatedAt, public, main branch, empty topics)', () => {
    const line = formatRepoLine(base);
    expect(line).not.toContain('https://github.com'); // url derivable
    expect(line).not.toContain('2026-06-08'); // updatedAt dropped (pushed covers freshness)
    expect(line).not.toContain('public'); // default visibility
    expect(line).not.toContain('@main'); // default branch
    expect(line).not.toContain('#'); // no topics
  });

  it('surfaces non-default branch, private, and topics when present', () => {
    const line = formatRepoLine({
      ...base,
      defaultBranch: 'develop',
      visibility: 'private',
      topics: ['mcp', 'ai', 'agents', 'extra', 'fifth'],
    } as typeof base);
    expect(line).toContain('@develop');
    expect(line).toContain('private');
    expect(line).toContain('#mcp,ai,agents,extra'); // capped at 4
    expect(line).not.toContain('fifth');
  });

  it('truncates long descriptions and collapses whitespace', () => {
    const line = formatRepoLine({
      ...base,
      description: 'x'.repeat(200) + '\n\nmore',
    } as typeof base);
    const desc = line.split(' | ').pop()!;
    expect(desc.length).toBeLessThanOrEqual(102);
    expect(desc).not.toContain('\n');
  });
});
