import { describe, expect, it } from 'vitest';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
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

async function withFixture(
  files: Record<string, string>,
  run: (dir: string) => Promise<void>
): Promise<void> {
  const dir = mkdtempSync(path.join(here, '.tmp-oql-'));
  try {
    for (const [name, content] of Object.entries(files)) {
      writeFileSync(path.join(dir, name), content);
    }
    await run(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

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
    expect(env.results.every(r => r.proofGrade === 'text')).toBe(true);
    expect(env.evidence.kind).toBe('proof');
    expect(env.provenance[0]?.backend).toBe('localSearchCode');
  });

  it('discovery view returns file rows (paths only)', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'OqlQuery' },
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

  it('maps extension selector tsx to an include glob, not langType', async () => {
    await withFixture(
      {
        'component.tsx': 'export const Component = () => <div>needle</div>;\n',
        'plain.ts': 'export const value = "needle";\n',
      },
      async fixture => {
        const env = single(
          await runOqlSearch({
            target: 'code',
            from: { kind: 'local', path: fixture },
            scope: { language: 'tsx' },
            where: { kind: 'text', value: 'needle' },
          })
        );

        expect(env.diagnostics.some(d => d.code === 'invalidQuery')).toBe(
          false
        );
        expect(env.results.map(r => path.basename(r.path))).toEqual([
          'component.tsx',
        ]);
      }
    );
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

  it('expands language selectors to known local file extensions', async () => {
    await withFixture(
      {
        'a.ts': 'export const a = 1;\n',
        'b.tsx': 'export const B = () => null;\n',
        'c.js': 'export const c = 1;\n',
      },
      async fixture => {
        const env = single(
          await runOqlSearch({
            target: 'files',
            from: { kind: 'local', path: fixture },
            scope: { language: 'typescript' },
          })
        );

        expect(env.results.map(r => path.basename(r.path)).sort()).toEqual([
          'a.ts',
          'b.tsx',
        ]);
      }
    );
  });

  it('honors local files include and exclude scope filters', async () => {
    await withFixture(
      {
        'a.tsx': 'export const A = () => null;\n',
        'b.tsx': 'export const B = () => null;\n',
        'skip.test.tsx': 'export const Skip = () => null;\n',
        'plain.ts': 'export const plain = 1;\n',
      },
      async fixture => {
        const env = single(
          await runOqlSearch({
            target: 'files',
            from: { kind: 'local', path: fixture },
            scope: {
              include: ['**/*.tsx'],
              exclude: ['**/*.test.tsx'],
            },
          })
        );

        expect(env.results.map(r => path.basename(r.path)).sort()).toEqual([
          'a.tsx',
          'b.tsx',
        ]);
      }
    );
  });

  it('honors structure pattern, sort, and limit filters', async () => {
    await withFixture(
      {
        'shared.ts': 'export const shared = 1;\n',
        'status.ts': 'export const status = 1;\n',
        'zebra.js': 'export const zebra = 1;\n',
      },
      async fixture => {
        const env = single(
          await runOqlSearch({
            target: 'structure',
            from: { kind: 'local', path: fixture },
            fetch: {
              tree: {
                maxDepth: 1,
                pattern: 's*.ts',
                filesOnly: true,
                sortBy: 'name',
              },
            },
            limit: 1,
          })
        );

        expect(env.results.map(r => path.basename(r.path))).toEqual([
          'shared.ts',
        ]);
      }
    );
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

    it('normalizes startLine-only ranges with context into executable content reads', async () => {
      const env = single(
        await runOqlSearch({
          target: 'content',
          from: { kind: 'local', path: path.join(OQL_SRC, 'types.ts') },
          fetch: {
            content: {
              range: { startLine: 12, contextLines: 2 },
              contentView: 'exact',
            },
          },
        })
      );

      expect(env.diagnostics.some(d => d.code === 'invalidQuery')).toBe(false);
      expect(env.results[0]?.kind).toBe('content');
      expect((env.results[0] as { content: string }).content).toContain(
        'OqlActiveTarget'
      );
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

  it('reserved target -> unsupported envelope with unsupportedTarget', async () => {
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

  it('dry-run surfaces GitHub adapter validation diagnostics', async () => {
    const env = single(
      await runOqlSearch(
        {
          target: 'code',
          from: { kind: 'github', repo: 'vercel/next.js' },
          // Multiple languages are genuinely lossy: GitHub code search cannot OR
          // them in one query, so the transform must surface a lossyTransform
          // diagnostic. A single language is expressible and is NOT lossy.
          scope: { language: ['typescript', 'tsx'] },
          where: { kind: 'text', value: 'createComponentTree' },
        },
        { dryRun: true }
      )
    );

    expect(env.results.length).toBe(0);
    expect(env.plan?.diagnostics.some(d => d.code === 'lossyTransform')).toBe(
      true
    );
    expect(env.diagnostics.some(d => d.code === 'lossyTransform')).toBe(true);
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
    expect(dirs.some(d => path.basename(d.path) === 'adapters')).toBe(true);
    expect(files.some(f => path.basename(f.path) === 'planner.ts')).toBe(true);
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

  it('all[] preserves directory entryType for field predicates', async () => {
    const env = single(
      await runOqlSearch({
        target: 'files',
        from: { kind: 'local', path: OQL_SRC },
        where: {
          kind: 'all',
          of: [
            {
              kind: 'field',
              field: 'entryType',
              op: '=',
              value: 'directory',
            },
            {
              kind: 'field',
              field: 'basename',
              op: 'glob',
              value: '*adapters*',
            },
          ],
        },
      })
    );
    expect(env.evidence.kind).toBe('proof');
    expect(env.results).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          path: 'adapters',
          entryType: 'directory',
        }),
      ])
    );
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

  it('not(content leaf) uses the single-call filesWithoutMatch fast path', async () => {
    await withFixture(
      {
        'has.txt': 'needle\nother\n',
        'miss.txt': 'other\n',
      },
      async fixture => {
        const env = single(
          await runOqlSearch({
            target: 'files',
            from: { kind: 'local', path: fixture },
            where: {
              kind: 'not',
              predicate: { kind: 'text', value: 'needle' },
            },
          })
        );

        expect(env.results.map(r => path.basename(r.path))).toEqual([
          'miss.txt',
        ]);
        expect(env.provenance.map(p => p.backend)).toEqual(['localSearchCode']);
      }
    );
  });
});

describe('OQL negated discovery over target:"code"', () => {
  it('not(content leaf) is file-level absence, not line-level inversion', async () => {
    await withFixture(
      {
        'mixed.ts': 'needle\nother\n',
        'miss.ts': 'other\n',
      },
      async fixture => {
        const env = single(
          await runOqlSearch({
            target: 'code',
            from: { kind: 'local', path: fixture },
            view: 'discovery',
            where: {
              kind: 'not',
              predicate: { kind: 'text', value: 'needle' },
            },
          })
        );

        expect(env.results.map(r => path.basename(r.path))).toEqual([
          'miss.ts',
        ]);
        expect(env.provenance.map(p => p.backend)).toEqual(['localSearchCode']);
      }
    );
  });
});

describe('controls.budget.maxFiles caps files (search --max-files parity)', () => {
  it('budget.maxFiles alone (no controls.search) limits distinct files', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'diagnostic' },
        controls: { budget: { maxFiles: 2 } },
      })
    );
    const files = new Set(env.results.map(r => r.path));
    expect(files.size).toBeLessThanOrEqual(2);
    expect(files.size).toBeGreaterThan(0);
  });
});

describe('structural zero-match self-correction (the #1 agent failure)', () => {
  it('a too-specific pattern (0 matches) hands back a repair steering to a rule', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        // valid pattern, but no function has this bogus return type → 0 matches
        where: {
          kind: 'structural',
          lang: 'ts',
          pattern: 'function $N($$$A): ZzNoSuchReturnType000 { $$$B }',
        },
      })
    );
    expect(env.results.length).toBe(0);
    const d = env.diagnostics.find(x => x.code === 'zeroMatches');
    expect(d).toBeDefined();
    expect(d!.blocksAnswer).toBe(false);
    expect(d!.repair?.message).toMatch(/rule/i);
  });

  it('a rule that matches does NOT emit the structural zero-match hint', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: {
          kind: 'structural',
          lang: 'ts',
          rule: {
            kind: 'function_declaration',
            has: { pattern: 'buildEnvelope' },
          },
        },
      })
    );
    expect(env.results.length).toBeGreaterThan(0);
    expect(env.diagnostics.some(x => x.code === 'zeroMatches')).toBe(false);
  });
});
