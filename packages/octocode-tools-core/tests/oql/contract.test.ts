import { describe, expect, it } from 'vitest';

import {
  OqlCanonicalInputSchema,
  OqlQuerySchema,
} from '../../src/oql/schema.js';
import { normalizeInput, normalizeQuery } from '../../src/oql/normalize.js';
import { planQuery } from '../../src/oql/planner.js';
import { OqlValidationError } from '../../src/oql/diagnostics.js';
import type { OqlQueryV1 } from '../../src/oql/types.js';

/** Canonical examples copied verbatim from OCTOCODE_QUERY_LANGUAGE.md §examples. */
const CANONICAL_EXAMPLES: Record<string, unknown> = {
  'local-literal-search': {
    schema: 'oql/v1',
    target: 'code',
    from: { kind: 'local', path: './packages/octocode/src' },
    scope: { language: ['ts'] },
    where: { kind: 'text', value: 'runCLI' },
    select: ['path', 'line', 'snippet', 'next.fetch'],
    view: 'paginated',
    limit: 25,
  },
  'local-regex-pcre2': {
    schema: 'oql/v1',
    target: 'code',
    from: { kind: 'local', path: './src' },
    where: { kind: 'regex', value: 'function\\s+(?=handle)', dialect: 'pcre2' },
    view: 'detailed',
  },
  'local-structural-search': {
    schema: 'oql/v1',
    target: 'code',
    from: { kind: 'local', path: './src' },
    where: {
      kind: 'structural',
      lang: 'ts',
      pattern: 'class $NAME { $$$BODY }',
    },
    select: ['path', 'line', 'snippet', 'metavars', 'next.fetch'],
    view: 'detailed',
  },
  'local-structural-rule': {
    schema: 'oql/v1',
    target: 'code',
    from: { kind: 'local', path: './src' },
    where: {
      kind: 'structural',
      lang: 'ts',
      rule: {
        pattern: 'await $X',
        not: { inside: { kind: 'try_statement', stopBy: 'end' } },
      },
    },
    select: ['path', 'line', 'snippet', 'metavars', 'next.fetch'],
    view: 'detailed',
  },
  'github-provider-search': {
    schema: 'oql/v1',
    target: 'code',
    from: { kind: 'github', repo: 'facebook/react', ref: 'main' },
    scope: { path: 'packages/react', language: ['js'] },
    where: { kind: 'text', value: 'useEffect' },
    materialize: { mode: 'never' },
    select: ['repo', 'path', 'line', 'snippet', 'next.fetch'],
    view: 'paginated',
  },
  'github-structural-materialized': {
    schema: 'oql/v1',
    target: 'code',
    from: { kind: 'github', repo: 'facebook/react', ref: 'main' },
    scope: { path: 'packages/react', language: ['js'] },
    where: { kind: 'structural', lang: 'js', pattern: 'useEffect($$$ARGS)' },
    materialize: { mode: 'auto', strategy: 'subtree' },
    select: ['repo', 'localPath', 'path', 'line', 'snippet', 'next.fetch'],
    view: 'detailed',
    controls: {
      budget: {
        maxFiles: 500,
        maxMaterializedBytes: 50000000,
        timeoutMs: 30000,
      },
    },
    explain: true,
  },
  'content-fetch': {
    schema: 'oql/v1',
    target: 'content',
    from: { kind: 'local', path: './src/index.ts' },
    fetch: {
      content: { range: { startLine: 40, endLine: 90 }, contentView: 'exact' },
    },
    select: ['path', 'content', 'next.search'],
  },
  structure: {
    schema: 'oql/v1',
    target: 'structure',
    from: { kind: 'github', repo: 'facebook/react', ref: 'main' },
    scope: { path: 'packages/react' },
    fetch: { tree: { maxDepth: 2, includeSizes: true } },
    view: 'discovery',
  },
  files: {
    schema: 'oql/v1',
    target: 'files',
    from: { kind: 'local', path: './packages' },
    scope: { language: ['ts'], excludeDir: ['node_modules', 'dist'] },
    where: {
      kind: 'field',
      field: 'basename',
      op: 'regex',
      value: '^(index|main)\\.(ts|tsx)$',
    },
    select: ['path', 'size', 'modified', 'next.fetch'],
    view: 'discovery',
  },
};

describe('OQL gate 1: every canonical example parses as oql/v1', () => {
  for (const [name, example] of Object.entries(CANONICAL_EXAMPLES)) {
    it(`parses ${name}`, () => {
      const result = OqlCanonicalInputSchema.safeParse(example);
      expect(result.success).toBe(true);
    });
  }
});

describe('OQL gate 1b: canonical examples survive normalization', () => {
  for (const [name, example] of Object.entries(CANONICAL_EXAMPLES)) {
    it(`normalizes ${name} to a valid canonical query`, () => {
      const normalized = normalizeInput(example as never);
      expect(OqlCanonicalInputSchema.safeParse(normalized).success).toBe(true);
    });
  }
});

describe('OQL gate 2: sugar normalizes to documented canonical shape', () => {
  it('repo+path+pattern+lang -> github structural with auto/subtree materialize', () => {
    const sugar = {
      repo: 'facebook/react',
      path: 'packages/react',
      pattern: 'useEffect($$$ARGS)',
      lang: 'js',
    };
    const normalized = normalizeQuery(sugar as never);
    expect(normalized).toEqual({
      schema: 'oql/v1',
      target: 'code',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react' },
      where: { kind: 'structural', lang: 'js', pattern: 'useEffect($$$ARGS)' },
      materialize: { mode: 'auto', strategy: 'subtree' },
      view: 'paginated',
    });
  });

  it('top-level text + local path -> local code text predicate', () => {
    const normalized = normalizeQuery({
      path: './src',
      text: 'runCLI',
    } as never);
    expect(normalized.target).toBe('code');
    expect(normalized.from).toEqual({ kind: 'local', path: './src' });
    expect(normalized.where).toEqual({ kind: 'text', value: 'runCLI' });
  });

  it('owner + repo name -> repo:"owner/name"', () => {
    const normalized = normalizeQuery({
      owner: 'facebook',
      repo: 'react',
      text: 'x',
    } as never);
    expect(normalized.from).toEqual({ kind: 'github', repo: 'facebook/react' });
  });

  it('minify -> fetch.content.contentView', () => {
    const normalized = normalizeQuery({
      path: './a.ts',
      target: 'content',
      minify: 'symbols',
    } as never);
    expect(normalized.fetch?.content?.contentView).toBe('symbols');
  });
});

describe('OQL gate 3 & 14 & 16 & 18: boolean/invert/legacy sugar', () => {
  it('and/or -> all/any', () => {
    const n = normalizeQuery({
      path: './src',
      and: [
        { kind: 'text', value: 'a' },
        { kind: 'text', value: 'b' },
      ],
    } as never);
    expect(n.where?.kind).toBe('all');
  });

  it('xor -> any(all(A,not(B)),all(not(A),B))', () => {
    const n = normalizeQuery({
      path: './src',
      xor: [
        { kind: 'text', value: 'a' },
        { kind: 'text', value: 'b' },
      ],
    } as never);
    expect(n.where).toEqual({
      kind: 'any',
      of: [
        {
          kind: 'all',
          of: [
            { kind: 'text', value: 'a' },
            { kind: 'not', predicate: { kind: 'text', value: 'b' } },
          ],
        },
        {
          kind: 'all',
          of: [
            { kind: 'not', predicate: { kind: 'text', value: 'a' } },
            { kind: 'text', value: 'b' },
          ],
        },
      ],
    });
  });

  it('invert -> not(predicate)', () => {
    const n = normalizeQuery({
      path: './src',
      text: 'a',
      invert: true,
    } as never);
    expect(n.where).toEqual({
      kind: 'not',
      predicate: { kind: 'text', value: 'a' },
    });
  });

  it('filesWithoutMatch -> target:"files" + not(predicate)', () => {
    const n = normalizeQuery({
      path: './src',
      text: 'TODO',
      filesWithoutMatch: true,
    } as never);
    expect(n.target).toBe('files');
    expect(n.where).toEqual({
      kind: 'not',
      predicate: { kind: 'text', value: 'TODO' },
    });
  });

  it('filesOnly -> discovery view + path projection', () => {
    const n = normalizeQuery({
      path: './src',
      text: 'a',
      filesOnly: true,
    } as never);
    expect(n.view).toBe('discovery');
    expect(n.select).toEqual(['path', 'next.fetch']);
  });
});

describe('OQL gate 3: unknown fields fail', () => {
  it('rejects an unknown top-level field', () => {
    expect(() =>
      normalizeQuery({ path: './src', text: 'a', bogusField: 1 } as never)
    ).toThrowError(OqlValidationError);
  });
});

describe('OQL gate 4: reserved (V3) targets fail with unsupportedTarget', () => {
  // V2 targets (semantics/repositories/packages/pullRequests/commits/artifacts/
  // diff) are now active; only V3 fixes/dataflow remain reserved.
  for (const target of ['fixes', 'dataflow']) {
    it(`rejects target:"${target}"`, () => {
      try {
        normalizeQuery({
          target,
          from: { kind: 'local', path: './x' },
        } as never);
        throw new Error('should have thrown');
      } catch (err) {
        expect(err).toBeInstanceOf(OqlValidationError);
        expect((err as OqlValidationError).diagnostics[0]?.code).toBe(
          'unsupportedTarget'
        );
      }
    });
  }
});

describe('OQL: ambiguous/impossible sugar', () => {
  it('both scope.path and top-level path -> ambiguousSugar', () => {
    try {
      normalizeQuery({
        from: { kind: 'local', path: './src' },
        scope: { path: 'a' },
        path: 'b',
        text: 'x',
      } as never);
      throw new Error('should have thrown');
    } catch (err) {
      expect((err as OqlValidationError).diagnostics[0]?.code).toBe(
        'ambiguousSugar'
      );
    }
  });

  it('structural pattern + rule together -> invalidQuery', () => {
    expect(() =>
      normalizeQuery({
        path: './src',
        lang: 'ts',
        pattern: 'foo()',
        rule: { pattern: 'bar()' },
      } as never)
    ).toThrowError(OqlValidationError);
  });

  it('target:"code" without predicate -> invalidQuery', () => {
    expect(() =>
      normalizeQuery({
        target: 'code',
        from: { kind: 'local', path: './x' },
      } as never)
    ).toThrowError(OqlValidationError);
  });
});

describe('OQL planner: predicate routing', () => {
  function plan(input: unknown) {
    const q = normalizeQuery(input as never) as OqlQueryV1;
    return planQuery(q, input);
  }

  it('local text -> PUSHDOWN localSearchCode', () => {
    const { plan: p, executable } = plan({ path: './src', text: 'x' });
    expect(executable).toBe(true);
    expect(p.nodes[0]?.route).toBe('PUSHDOWN');
    expect(p.nodes[0]?.backend).toBe('localSearchCode');
  });

  it('local structural -> PUSHDOWN localSearchCode', () => {
    const { plan: p } = plan({
      path: './src',
      lang: 'ts',
      pattern: 'class $N { $$$B }',
    });
    expect(p.nodes[0]?.route).toBe('PUSHDOWN');
  });

  it('GitHub text -> PUSHDOWN ghSearchCode (provider pushdown)', () => {
    const { plan: p, executable } = plan({
      repo: 'facebook/react',
      text: 'useEffect',
      materialize: 'never',
    });
    expect(executable).toBe(true);
    expect(p.nodes[0]?.backend).toBe('ghSearchCode');
    expect(p.nodes[0]?.route).toBe('PUSHDOWN');
  });

  it('GitHub structural + materialize auto -> ROUTE (gate 9)', () => {
    const { plan: p, executable } = plan({
      repo: 'facebook/react',
      path: 'packages/react',
      lang: 'js',
      pattern: 'useEffect($$$ARGS)',
      materialize: { mode: 'auto', strategy: 'subtree' },
    });
    expect(executable).toBe(true);
    expect(p.nodes[0]?.route).toBe('ROUTE');
    expect(p.materialization?.required).toBe(false);
  });

  it('GitHub structural + materialize never -> UNSUPPORTED (gate 8)', () => {
    const { plan: p, executable } = plan({
      repo: 'facebook/react',
      lang: 'js',
      pattern: 'useEffect($$$ARGS)',
      materialize: 'never',
    });
    expect(executable).toBe(false);
    expect(p.nodes[0]?.route).toBe('UNSUPPORTED');
    expect(
      p.diagnostics.some(d => d.code === 'materializationNotAllowed')
    ).toBe(true);
  });

  it('preserves all predicate nodes (invariant)', () => {
    const { plan: p } = plan({
      path: './src',
      and: [
        { kind: 'text', value: 'a' },
        { kind: 'regex', value: 'b' },
      ],
    });
    // all + 2 leaves = 3 nodes
    expect(p.nodes.length).toBe(3);
    expect(p.diagnostics.some(d => d.code === 'invalidQuery')).toBe(false);
  });

  it('explain shows applied defaults', () => {
    const { plan: p } = plan({ path: './src', text: 'x' });
    expect(p.defaults.schema).toBe('oql/v1');
    expect(p.defaults.view).toBe('paginated');
    expect(p.defaults.page).toBe(1);
  });
});

describe('OQL gate 21: plan truncation', () => {
  it('caps nodes at maxPlanNodes and emits planTruncated', () => {
    const q = normalizeQuery({
      path: './src',
      or: [
        { kind: 'text', value: 'a' },
        { kind: 'text', value: 'b' },
        { kind: 'text', value: 'c' },
      ],
      controls: { budget: { maxPlanNodes: 2 } },
    } as never) as OqlQueryV1;
    const { plan: p } = planQuery(q, {});
    expect(p.truncated).toBe(true);
    expect(p.nodes.length).toBe(2);
    expect(p.diagnostics.some(d => d.code === 'planTruncated')).toBe(true);
  });
});
