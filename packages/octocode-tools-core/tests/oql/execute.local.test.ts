import { describe, expect, it } from 'vitest';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { runOqlSearch } from '../../src/oql/run.js';
import {
  isBatchEnvelope,
  type OqlResultEnvelope,
} from '../../src/oql/types.js';

const here = path.dirname(fileURLToPath(import.meta.url));
// the OQL source dir — a stable corpus to search against
const OQL_SRC = path.resolve(here, '../../src/oql');

function single(
  result: Awaited<ReturnType<typeof runOqlSearch>>
): OqlResultEnvelope {
  if (isBatchEnvelope(result)) throw new Error('expected single envelope');
  return result;
}

describe('OQL local execution (target:"code")', () => {
  it('finds a literal string in the OQL source', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'runOqlSearch' },
        view: 'paginated',
      })
    );
    expect(env.results.length).toBeGreaterThan(0);
    expect(env.results.every(r => r.kind === 'code')).toBe(true);
    expect(env.evidence.kind).toBe('proof');
    expect(env.provenance[0]?.backend).toBe('localSearchCode');
  });

  it('discovery view returns file rows (paths only)', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'OqlQueryV1' },
        view: 'discovery',
      })
    );
    expect(env.results.length).toBeGreaterThan(0);
  });

  it('a non-matching query reports zeroMatches, not proof-of-presence', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'zzz_definitely_absent_token_zzz' },
      })
    );
    expect(env.results.length).toBe(0);
    expect(env.diagnostics.some(d => d.code === 'zeroMatches')).toBe(true);
  });
});

describe('OQL local execution (target:"structure" / "files")', () => {
  it('lists the tree of the OQL source dir', async () => {
    const env = single(
      await runOqlSearch({
        target: 'structure',
        from: { kind: 'local', path: OQL_SRC },
        fetch: { tree: { maxDepth: 1 } },
        view: 'discovery',
      })
    );
    expect(env.results.length).toBeGreaterThan(0);
    expect(env.results.every(r => r.kind === 'tree')).toBe(true);
  });

  it('finds files by basename glob', async () => {
    const env = single(
      await runOqlSearch({
        target: 'files',
        from: { kind: 'local', path: OQL_SRC },
        where: {
          kind: 'field',
          field: 'basename',
          op: 'glob',
          value: 'planner.ts',
        },
      })
    );
    expect(env.results.some(r => r.path.endsWith('planner.ts'))).toBe(true);
  });
});

describe('OQL local execution (target:"content")', () => {
  it('reads a bounded line range from a source file', async () => {
    const env = single(
      await runOqlSearch({
        target: 'content',
        from: { kind: 'local', path: path.join(OQL_SRC, 'types.ts') },
        fetch: {
          content: {
            range: { startLine: 1, endLine: 5 },
            contentView: 'exact',
          },
        },
      })
    );
    expect(env.results[0]?.kind).toBe('content');
    expect(
      (env.results[0] as { content: string }).content.length
    ).toBeGreaterThan(0);
  });
});

describe('OQL runner: validation + dry-run + GitHub routing', () => {
  it('invalid input returns an unsupported envelope (no throw)', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: './x' },
      } as never)
    );
    expect(env.evidence.kind).toBe('unsupported');
    expect(env.diagnostics.some(d => d.code === 'invalidQuery')).toBe(true);
  });

  it('reserved (V3) target -> unsupported envelope with unsupportedTarget', async () => {
    const env = single(
      await runOqlSearch({
        target: 'dataflow',
        from: { kind: 'local', path: '.' },
      } as never)
    );
    expect(env.evidence.kind).toBe('unsupported');
    expect(env.diagnostics.some(d => d.code === 'unsupportedTarget')).toBe(
      true
    );
  });

  it('dry-run returns a plan without executing', async () => {
    const env = single(
      await runOqlSearch(
        {
          target: 'code',
          from: { kind: 'local', path: OQL_SRC },
          where: { kind: 'text', value: 'runOqlSearch' },
        },
        { dryRun: true }
      )
    );
    expect(env.plan).toBeDefined();
    expect(env.plan?.normalized).toBeDefined();
    expect(env.results.length).toBe(0);
  });

  it('GitHub structural + materialize:never -> unsupported (no network)', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'github', repo: 'facebook/react' },
        where: {
          kind: 'structural',
          lang: 'js',
          pattern: 'useEffect($$$ARGS)',
        },
        materialize: { mode: 'never' },
      })
    );
    expect(env.evidence.kind).toBe('unsupported');
    expect(
      env.diagnostics.some(d => d.code === 'materializationNotAllowed')
    ).toBe(true);
  });
});

describe('OQL batch', () => {
  it('runs an independent batch and preserves per-query envelopes', async () => {
    const result = await runOqlSearch({
      queries: [
        {
          target: 'code',
          from: { kind: 'local', path: OQL_SRC },
          where: { kind: 'text', value: 'runOqlSearch' },
        },
        {
          target: 'files',
          from: { kind: 'local', path: OQL_SRC },
          where: { kind: 'field', field: 'extension', op: '=', value: 'ts' },
        },
      ],
    });
    if (!isBatchEnvelope(result)) throw new Error('expected batch');
    expect(result.children.length).toBe(2);
    expect(result.mode).toBe('independent');
    expect(result.children[0]?.envelope.results.length).toBeGreaterThan(0);
  });

  it('combine:"merge" across incompatible row kinds -> repair diagnostic', async () => {
    const result = await runOqlSearch({
      combine: 'merge',
      queries: [
        {
          target: 'code',
          from: { kind: 'local', path: OQL_SRC },
          where: { kind: 'text', value: 'runOqlSearch' },
        },
        {
          target: 'files',
          from: { kind: 'local', path: OQL_SRC },
          where: { kind: 'field', field: 'extension', op: '=', value: 'ts' },
        },
      ],
    });
    if (!isBatchEnvelope(result)) throw new Error('expected batch');
    expect(result.merged).toBeUndefined();
    expect(result.diagnostics.some(d => d.code === 'invalidQuery')).toBe(true);
  });
});

describe('OQL match truncation honesty (regression: 10/file cap was reported as proof)', () => {
  it('surfaces matchTruncated + partial evidence when a file is capped', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'diagnostic' },
      })
    );
    // OQL dir has files with >10 'diagnostic' matches → must report truncation,
    // and MUST NOT claim proof/complete.
    expect(env.diagnostics.some(d => d.code === 'matchTruncated')).toBe(true);
    expect(env.evidence.kind).not.toBe('proof');
    expect(env.evidence.complete).toBe(false);
    expect(env.evidence.answerReady).toBe(false);
  });

  it('still reports proof when no file is truncated', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'Oqlქ_nonexistent_or_rare' },
        controls: { search: { maxMatchesPerFile: 5000 } },
      })
    );
    expect(env.diagnostics.some(d => d.code === 'matchTruncated')).toBe(false);
  });
});

describe('OQL structure target correctness', () => {
  it('matches the filesystem and reports numeric byte sizes when includeSizes', async () => {
    const env = single(
      await runOqlSearch({
        target: 'structure',
        from: { kind: 'local', path: OQL_SRC },
        fetch: { tree: { maxDepth: 1, includeSizes: true } },
      })
    );
    const files = env.results.filter(r => r.entryType === 'file');
    const dirs = env.results.filter(r => r.entryType === 'directory');
    // OQL dir: 1 subdir (adapters) + the source files
    expect(dirs.some(d => d.path.endsWith('/adapters'))).toBe(true);
    expect(files.some(f => f.path.endsWith('/planner.ts'))).toBe(true);
    // file rows carry a numeric byte size; directory rows do not
    expect(
      files.every(f => typeof (f as { size?: number }).size === 'number')
    ).toBe(true);
    expect(env.evidence.kind).toBe('proof');
  });
});

describe('OQL boolean execution over target:"files" (improvement: was unsupportedBoolean)', () => {
  it('all[] = intersection of file sets', async () => {
    const env = single(
      await runOqlSearch({
        target: 'files',
        from: { kind: 'local', path: OQL_SRC },
        where: {
          kind: 'all',
          of: [
            { kind: 'text', value: 'searchContentRipgrep' },
            { kind: 'text', value: 'findFiles' },
          ],
        },
      })
    );
    expect(env.evidence.kind).toBe('proof');
    expect(env.results.length).toBeGreaterThan(0);
    expect(env.results.every(r => r.kind === 'file')).toBe(true);
    // only adapters/local.ts has both
    expect(env.results.every(r => r.path.endsWith('local.ts'))).toBe(true);
  });

  it('any[] = union; not = files-only complement (no directories)', async () => {
    const orEnv = single(
      await runOqlSearch({
        target: 'files',
        from: { kind: 'local', path: OQL_SRC },
        where: {
          kind: 'any',
          of: [
            { kind: 'text', value: 'structuralRuleToYaml' },
            { kind: 'text', value: 'coerceSizeToBytes' },
          ],
        },
      })
    );
    expect(orEnv.results.length).toBeGreaterThanOrEqual(2);

    const notEnv = single(
      await runOqlSearch({
        target: 'files',
        from: { kind: 'local', path: OQL_SRC },
        where: {
          kind: 'not',
          predicate: { kind: 'text', value: 'diagnostic' },
        },
      })
    );
    expect(notEnv.evidence.kind).toBe('proof');
    // never returns a directory entry
    expect(notEnv.results.every(r => r.entryType === 'file')).toBe(true);
    expect(notEnv.results.every(r => r.path.endsWith('.ts'))).toBe(true);
  });
});
