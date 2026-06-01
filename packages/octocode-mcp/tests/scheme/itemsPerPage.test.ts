/**
 * itemsPerPage is the ONE page-size field on every tool (no aliases, no legacy
 * `limit`). githubAPILimit is the GitHub-only raw per_page override. Verifies
 * the resolver precedence + that each tool exposes `itemsPerPage` and the old
 * per-tool knob names are gone.
 */
import { describe, it, expect } from 'vitest';
import {
  resolveGithubPerPage,
  DEFAULT_ITEMS_PER_PAGE,
} from '../../src/scheme/localSchemaOverlay.js';
import {
  GitHubReposSearchBulkQueryLocalSchema,
  GitHubCodeSearchBulkQueryLocalSchema,
  GitHubViewRepoStructureBulkQueryLocalSchema,
  PackageSearchBulkQueryLocalSchema,
} from '../../src/scheme/remoteSchemaOverlay.js';
import {
  BulkRipgrepQuerySchema,
  BulkFindFilesSchema,
  BulkViewStructureSchema,
} from '../../src/scheme/localSchemaOverlay.js';

describe('resolveGithubPerPage precedence', () => {
  it('githubAPILimit wins over itemsPerPage', () => {
    expect(resolveGithubPerPage({ githubAPILimit: 50, itemsPerPage: 20 })).toBe(
      50
    );
  });
  it('itemsPerPage drives per_page when githubAPILimit is absent', () => {
    expect(resolveGithubPerPage({ itemsPerPage: 20 })).toBe(20);
  });
  it('falls back to the default when nothing is set', () => {
    expect(resolveGithubPerPage({})).toBe(DEFAULT_ITEMS_PER_PAGE);
  });
});

describe('GitHub search: itemsPerPage default + githubAPILimit, no legacy limit', () => {
  it('defaults itemsPerPage to 20 and exposes no `limit` key', () => {
    const parsed = GitHubReposSearchBulkQueryLocalSchema.parse({
      queries: [{ keywordsToSearch: ['x'] }],
    });
    const q = parsed.queries[0] as Record<string, unknown>;
    expect(q.itemsPerPage).toBe(20);
    expect('limit' in q).toBe(false);
  });

  it('accepts githubAPILimit and bounds it to 100', () => {
    const parsed = GitHubReposSearchBulkQueryLocalSchema.parse({
      queries: [{ keywordsToSearch: ['x'], githubAPILimit: 999 }],
    });
    const q = parsed.queries[0] as Record<string, unknown>;
    expect(q.githubAPILimit).toBe(100);
    expect(resolveGithubPerPage(q)).toBe(100);
  });

  it('a legacy `limit` is dropped (no alias) — itemsPerPage default still applies', () => {
    const parsed = GitHubCodeSearchBulkQueryLocalSchema.parse({
      queries: [{ keywordsToSearch: ['x'], limit: 42 }],
    });
    const q = parsed.queries[0] as Record<string, unknown>;
    expect('limit' in q).toBe(false);
    expect('githubAPILimit' in q).toBe(false);
    expect(q.itemsPerPage).toBe(20);
  });

  it('caps itemsPerPage at 100', () => {
    const parsed = GitHubCodeSearchBulkQueryLocalSchema.parse({
      queries: [{ keywordsToSearch: ['x'], itemsPerPage: 500 }],
    });
    const q = parsed.queries[0] as Record<string, unknown>;
    expect(q.itemsPerPage).toBe(100);
  });
});

describe('itemsPerPage is the page-size field on every tool (old names gone)', () => {
  const q0 = (
    schema: { parse: (v: unknown) => { queries: unknown[] } },
    query: unknown
  ) => schema.parse({ queries: [query] }).queries[0] as Record<string, unknown>;

  it('ripgrep: itemsPerPage=files (top-level), matchesPerFile is the inner knob; legacy names gone', () => {
    const q = q0(BulkRipgrepQuerySchema, {
      pattern: 'x',
      path: '.',
      itemsPerPage: 5,
      matchesPerFile: 3,
    });
    expect(q.itemsPerPage).toBe(5); // files = top-level atomic item
    expect(q.matchesPerFile).toBe(3); // matches-per-file = inner axis
    expect('matchesPerPage' in q).toBe(false);
    expect('filesPerPage' in q).toBe(false); // replaced by itemsPerPage
    expect('filePageNumber' in q).toBe(false); // replaced by unified `page`
  });

  it('localFindFiles: itemsPerPage present, filesPerPage gone', () => {
    const q = q0(BulkFindFilesSchema, {
      path: '.',
      name: '*.ts',
      itemsPerPage: 7,
    });
    expect(q.itemsPerPage).toBe(7);
    expect('filesPerPage' in q).toBe(false);
  });

  it('localViewStructure: itemsPerPage present, entriesPerPage gone', () => {
    const q = q0(BulkViewStructureSchema, { path: '.', itemsPerPage: 8 });
    expect(q.itemsPerPage).toBe(8);
    expect('entriesPerPage' in q).toBe(false);
  });

  it('githubViewRepoStructure: itemsPerPage present, entriesPerPage gone', () => {
    const q = q0(GitHubViewRepoStructureBulkQueryLocalSchema, {
      owner: 'o',
      repo: 'r',
      itemsPerPage: 9,
    });
    expect(q.itemsPerPage).toBe(9);
    expect('entriesPerPage' in q).toBe(false);
  });

  it('packageSearch: itemsPerPage present, searchLimit gone', () => {
    const q = q0(PackageSearchBulkQueryLocalSchema, {
      name: 'react',
      itemsPerPage: 6,
    });
    expect(q.itemsPerPage).toBe(6);
    expect('searchLimit' in q).toBe(false);
  });
});
