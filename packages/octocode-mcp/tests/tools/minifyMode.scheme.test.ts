import { describe, it, expect } from 'vitest';
import { z } from 'zod';
import {
  FileContentQueryBaseLocalSchema,
  FileContentQueryLocalSchema,
} from '../../../octocode-tools-core/src/tools/github_fetch_content/scheme.js';
import { LocalFetchContentQuerySchema } from '../../../octocode-tools-core/src/tools/local_fetch_content/scheme.js';
import { GitHubPullRequestSearchQueryLocalSchema } from '../../../octocode-tools-core/src/tools/github_search_pull_requests/scheme.js';

const GH_BASE = { owner: 'o', repo: 'r', path: 'src/a.ts' };
const LOCAL_BASE = { path: 'src/a.ts' };
const PR_BASE = { prNumber: 1, owner: 'o', repo: 'r' };

function parseMinify(schema: z.ZodTypeAny, input: Record<string, unknown>) {
  const result = schema.safeParse(input);
  expect(result.success, JSON.stringify(result.error?.issues)).toBe(true);
  return (result.data as { minify?: unknown }).minify;
}

describe('minify enum — ghGetFileContent scheme', () => {
  it("defaults to 'standard' when omitted (schema-level default — comment-stripped view)", () => {
    expect(parseMinify(FileContentQueryLocalSchema, GH_BASE)).toBe('standard');
  });

  it.each(['none', 'standard', 'symbols'])('accepts "%s"', value => {
    expect(
      parseMinify(FileContentQueryLocalSchema, { ...GH_BASE, minify: value })
    ).toBe(value);
  });

  it('rejects boolean minify:true', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...GH_BASE,
      minify: true,
    });
    expect(result.success).toBe(false);
  });

  it('rejects boolean minify:false', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...GH_BASE,
      minify: false,
    });
    expect(result.success).toBe(false);
  });

  it('rejects unknown minify values', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...GH_BASE,
      minify: 'skeleton',
    });
    expect(result.success).toBe(false);
  });

  it('no longer exposes signaturesOnly in the schema', () => {
    const json = z.toJSONSchema(FileContentQueryBaseLocalSchema) as {
      properties?: Record<string, unknown>;
    };
    expect(json.properties).toBeDefined();
    expect(Object.keys(json.properties!)).not.toContain('signaturesOnly');
    expect(Object.keys(json.properties!)).toContain('minify');
  });
});

describe('minify enum — localGetFileContent scheme', () => {
  it("defaults to 'standard' when omitted (schema-level default — comment-stripped view)", () => {
    expect(parseMinify(LocalFetchContentQuerySchema, LOCAL_BASE)).toBe(
      'standard'
    );
  });

  it.each(['none', 'standard', 'symbols'])('accepts "%s"', value => {
    expect(
      parseMinify(LocalFetchContentQuerySchema, {
        ...LOCAL_BASE,
        minify: value,
      })
    ).toBe(value);
  });

  it('rejects boolean minify values', () => {
    expect(
      LocalFetchContentQuerySchema.safeParse({
        ...LOCAL_BASE,
        minify: true,
      }).success
    ).toBe(false);
    expect(
      LocalFetchContentQuerySchema.safeParse({
        ...LOCAL_BASE,
        minify: false,
      }).success
    ).toBe(false);
  });

  it('rejects unknown minify values', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
      ...LOCAL_BASE,
      minify: 'full',
    });
    expect(result.success).toBe(false);
  });

  it('no longer exposes signaturesOnly in the schema', () => {
    const json = z.toJSONSchema(LocalFetchContentQuerySchema) as {
      properties?: Record<string, unknown>;
    };
    expect(json.properties).toBeDefined();
    expect(Object.keys(json.properties!)).not.toContain('signaturesOnly');
    expect(Object.keys(json.properties!)).toContain('minify');
  });
});

describe('minify enum — ghHistoryResearch scheme', () => {
  it("defaults to 'standard' when omitted (schema-level default, token-saving patch view)", () => {
    expect(parseMinify(GitHubPullRequestSearchQueryLocalSchema, PR_BASE)).toBe(
      'standard'
    );
  });

  it.each(['none', 'standard'])('accepts "%s"', value => {
    expect(
      parseMinify(GitHubPullRequestSearchQueryLocalSchema, {
        ...PR_BASE,
        minify: value,
      })
    ).toBe(value);
  });

  it('rejects "symbols" — PR patches have no skeleton mode', () => {
    const result = GitHubPullRequestSearchQueryLocalSchema.safeParse({
      ...PR_BASE,
      minify: 'symbols',
    });
    expect(result.success).toBe(false);
  });

  it('rejects boolean minify values', () => {
    expect(
      GitHubPullRequestSearchQueryLocalSchema.safeParse({
        ...PR_BASE,
        minify: true,
      }).success
    ).toBe(false);
    expect(
      GitHubPullRequestSearchQueryLocalSchema.safeParse({
        ...PR_BASE,
        minify: false,
      }).success
    ).toBe(false);
  });
});
