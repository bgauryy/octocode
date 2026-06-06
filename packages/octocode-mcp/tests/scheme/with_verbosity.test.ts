import { describe, it, expect, expectTypeOf } from 'vitest';

import {
  BulkRipgrepQuerySchema,
  optionalMetaFields,
} from '../../src/scheme/localSchemaOverlay.js';
import { BulkCloneRepoLocalSchema } from '../../src/scheme/remoteSchemaOverlay.js';
import type {
  WithVerbosity,
  WithLocalOverlay,
  RipgrepQuery,
  FindFilesQuery,
} from '../../src/scheme/localSchemaOverlay.js';

describe('WithVerbosity<T> generic', () => {
  it('adds optional verbose boolean field to T', () => {
    type Wrapped = WithVerbosity<{ name: string }>;
    expectTypeOf<Wrapped>().toMatchObjectType<{
      name: string;
      verbose?: boolean;
    }>();
  });
});

describe('WithLocalOverlay<T> generic', () => {
  it('adds query-meta fields plus verbose boolean', () => {
    type Wrapped = WithLocalOverlay<{ name: string }>;
    expectTypeOf<Wrapped>().toMatchObjectType<{
      name: string;
      id?: string;
      mainResearchGoal?: string;
      researchGoal?: string;
      reasoning?: string;
      verbose?: boolean;
    }>();
  });
});

describe('base query metadata fields', () => {
  it('declares the boolean verbose control on the shared base fields', () => {
    expect(Object.keys(optionalMetaFields)).toEqual(
      expect.arrayContaining(['verbose'])
    );
  });
});

describe('per-tool query types compose WithLocalOverlay', () => {
  it('preserves verbose:false at the bulk schema boundary (resolved at read time)', () => {
    const parsed = BulkRipgrepQuerySchema.parse({
      queries: [{ pattern: 'foo', path: '.', verbose: false }],
    });
    expect(parsed.queries[0]?.verbose).toBe(false);
  });

  it('preserves verbose:true; verbose is the only detail switch', () => {
    const parsed = BulkRipgrepQuerySchema.parse({
      queries: [
        { pattern: 'foo', path: '.', verbose: true },
        { pattern: 'bar', path: '.', verbose: false },
      ],
    });
    expect(parsed.queries[0]?.verbose).toBe(true);
    expect(parsed.queries[1]?.verbose).toBe(false);
  });

  it('preserves verbose for githubCloneRepo as part of the all-tools contract', () => {
    const parsed = BulkCloneRepoLocalSchema.parse({
      queries: [{ owner: 'octo', repo: 'repo', verbose: false }],
    });
    expect(parsed.queries[0]?.verbose).toBe(false);
  });

  it('FindFilesQuery exposes id/mainResearchGoal', () => {
    const q: FindFilesQuery = { path: '.', id: 'q1' };
    expectTypeOf(q.id).toMatchTypeOf<string | undefined>();
  });

  it('RipgrepQuery exposes verbose boolean', () => {
    const q: RipgrepQuery = { pattern: 'foo', path: '.', verbose: true };
    expectTypeOf(q.verbose).toMatchTypeOf<boolean | undefined>();
  });
});
