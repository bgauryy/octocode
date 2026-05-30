/**
 * Bulk resilience: a mutex-violating query in a bulk must NOT reject the whole
 * batch. The registered (bulk) schema accepts the batch; the per-query mutex is
 * enforced at the executor, returning a per-query error while valid siblings
 * still run. This honors the documented bulk contract: "one errored entry must
 * not block the others."
 *
 * The STRICT per-query schemas keep the mutex (see super_refine.test.ts) — the
 * executors validate each query against them and emit a per-query error. Only
 * the bulk-envelope schemas are relaxed so MCP input-validation accepts the
 * call instead of rejecting it wholesale with -32602.
 */
import { describe, it, expect } from 'vitest';
import {
  BulkFetchContentQuerySchema,
  BulkRipgrepQuerySchema,
} from '../../src/scheme/localSchemaOverlay.js';
import { FileContentBulkQueryLocalSchema } from '../../src/scheme/remoteSchemaOverlay.js';

describe('bulk schemas defer mutex to per-query (no whole-batch rejection)', () => {
  it('localGetFileContent bulk accepts a mutex-violating query alongside valid ones', () => {
    const r = BulkFetchContentQuerySchema.safeParse({
      queries: [
        { path: 'a.ts', fullContent: true, matchString: 'x' }, // mutex violation
        { path: 'b.ts', startLine: 1, endLine: 5 }, // valid sibling
      ],
    });
    expect(r.success).toBe(true);
  });

  it('localSearchCode bulk accepts a mutex-violating query alongside valid ones', () => {
    const r = BulkRipgrepQuerySchema.safeParse({
      queries: [
        { pattern: 'x', path: '/r', filesOnly: true, filesWithoutMatch: true }, // mutex
        { pattern: 'y', path: '/r' }, // valid sibling
      ],
    });
    expect(r.success).toBe(true);
  });

  it('githubGetFileContent bulk accepts a mutex-violating query alongside valid ones', () => {
    const r = FileContentBulkQueryLocalSchema.safeParse({
      queries: [
        {
          owner: 'o',
          repo: 'r',
          path: 'a.ts',
          fullContent: true,
          matchString: 'x',
        }, // mutex
        { owner: 'o', repo: 'r', path: 'b.ts', startLine: 1, endLine: 5 }, // valid
      ],
    });
    expect(r.success).toBe(true);
  });
});
