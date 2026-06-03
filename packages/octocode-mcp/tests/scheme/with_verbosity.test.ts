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
  FetchContentQuery,
  ViewStructureQuery,
  Verbosity,
} from '../../src/scheme/localSchemaOverlay.js';

describe('WithVerbosity<T> generic', () => {
  it('adds optional verbosity field to T', () => {
    type Wrapped = WithVerbosity<{ name: string }>;
    expectTypeOf<Wrapped>().toMatchObjectType<{
      name: string;
      verbosity?: Verbosity;
      verbose?: boolean;
    }>();
  });
});

describe('WithLocalOverlay<T> generic', () => {
  it('adds query-meta fields plus verbosity', () => {
    type Wrapped = WithLocalOverlay<{ name: string }>;
    expectTypeOf<Wrapped>().toMatchObjectType<{
      name: string;
      id?: string;
      mainResearchGoal?: string;
      researchGoal?: string;
      reasoning?: string;
      verbosity?: Verbosity;
      verbose?: boolean;
    }>();
  });
});

describe('base query metadata fields', () => {
  it('declares the boolean verbose control on the shared base fields', () => {
    expect(Object.keys(optionalMetaFields)).toEqual(
      expect.arrayContaining(['verbose', 'verbosity'])
    );
  });
});

describe('per-tool query types compose WithLocalOverlay', () => {
  it('RipgrepQuery exposes verbosity', () => {
    const q: RipgrepQuery = {
      pattern: 'foo',
      path: '.',
      verbosity: 'compact',
    };
    expectTypeOf(q.verbosity).toMatchTypeOf<Verbosity | undefined>();
  });
  it('preserves verbose:false at the bulk schema boundary (resolved at read time)', () => {
    // z.preprocess normalization was removed: verbose is preserved as-is in
    // the parsed output and resolved lazily by verbosity.ts readVerbosity().
    const parsed = BulkRipgrepQuerySchema.parse({
      queries: [{ pattern: 'foo', path: '.', verbose: false }],
    });
    expect(parsed.queries[0]?.verbose).toBe(false);
    // verbosity is undefined when not explicitly set; isConcise/isCompact both return false (trimming disabled)
    expect(parsed.queries[0]?.verbosity).toBeUndefined();
  });
  it('preserves verbose:true; explicit verbosity takes precedence', () => {
    const parsed = BulkRipgrepQuerySchema.parse({
      queries: [
        { pattern: 'foo', path: '.', verbose: true },
        {
          pattern: 'bar',
          path: '.',
          verbose: false,
          verbosity: 'concise',
        },
      ],
    });
    // verbose:true is preserved; verbosity is not injected at schema parse time
    expect(parsed.queries[0]?.verbose).toBe(true);
    expect(parsed.queries[0]?.verbosity).toBeUndefined();
    // explicit verbosity takes precedence and is untouched
    expect(parsed.queries[1]?.verbosity).toBe('concise');
  });
  it('preserves verbose for githubCloneRepo as part of the all-tools contract', () => {
    const parsed = BulkCloneRepoLocalSchema.parse({
      queries: [{ owner: 'octo', repo: 'repo', verbose: false }],
    });
    expect(parsed.queries[0]?.verbose).toBe(false);
    expect(parsed.queries[0]?.verbosity).toBeUndefined();
  });
  it('FindFilesQuery exposes id/mainResearchGoal', () => {
    const q: FindFilesQuery = { path: '.', id: 'q1' };
    expectTypeOf(q.id).toMatchTypeOf<string | undefined>();
  });
  it('FetchContentQuery exposes verbosity', () => {
    const q: FetchContentQuery = { path: 'a', verbosity: 'concise' };
    expectTypeOf(q.verbosity).toMatchTypeOf<Verbosity | undefined>();
  });
  it('ViewStructureQuery exposes verbosity', () => {
    const q: ViewStructureQuery = { path: '.', verbosity: 'basic' };
    expectTypeOf(q.verbosity).toMatchTypeOf<Verbosity | undefined>();
  });
});
