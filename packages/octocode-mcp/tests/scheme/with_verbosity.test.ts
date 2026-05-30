import { describe, it, expectTypeOf } from 'vitest';

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
    }>();
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
