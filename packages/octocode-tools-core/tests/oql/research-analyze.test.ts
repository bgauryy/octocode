import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

import { runOqlSearch } from '../../src/oql/run.js';
import { analyzeResearchFlow } from '../../src/oql/research/analyze.js';

async function fixture(): Promise<string> {
  const root = await mkdtemp(path.join(tmpdir(), 'octocode-research-'));
  await mkdir(path.join(root, 'src'), { recursive: true });
  await writeFile(
    path.join(root, 'package.json'),
    JSON.stringify(
      {
        name: 'fixture',
        main: 'src/index.ts',
        dependencies: {
          lodash: '^1.0.0',
          duplicate: '^1.0.0',
          unused: '^1.0.0',
        },
        devDependencies: {
          duplicate: '^1.0.0',
        },
      },
      null,
      2
    )
  );
  await writeFile(
    path.join(root, 'src/index.ts'),
    [
      "import { used } from './used';",
      "import lodash from 'lodash';",
      "import missing from 'missing-package';",
      'export const entryValue = used() + lodash.size([missing]);',
    ].join('\n')
  );
  await writeFile(
    path.join(root, 'src/used.ts'),
    ['export function used() {', '  return 1;', '}'].join('\n')
  );
  await writeFile(
    path.join(root, 'src/dead.ts'),
    ['export const dead = 1;'].join('\n')
  );
  await writeFile(
    path.join(root, 'src/dead-user.ts'),
    ["import { dead } from './dead';", 'export const onlyDead = dead;'].join(
      '\n'
    )
  );
  return root;
}

describe('smart OQL research analyzer', () => {
  it('builds a reachability/dependency research result from local evidence', async () => {
    const root = await fixture();
    try {
      const result = await analyzeResearchFlow({
        root,
        goal: 'find unused files, transitive dead exports, and package drift',
      });

      expect(result.intent).toBe('reachability');
      expect(result.flow.map(step => step.id)).toEqual(
        expect.arrayContaining([
          'orient',
          'manifest-graph',
          'symbol-inventory',
          'reference-proof',
          'dependency-audit',
        ])
      );
      expect(result.summary.unusedFiles).toBeGreaterThanOrEqual(2);
      expect(result.files.map(file => file.file)).toEqual(
        expect.arrayContaining(['src/dead.ts', 'src/dead-user.ts'])
      );
      expect(result.dependencies).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            kind: 'unlistedDependency',
            packageName: 'missing-package',
          }),
          expect.objectContaining({
            kind: 'unusedDependency',
            packageName: 'unused',
          }),
          expect.objectContaining({
            kind: 'duplicateDependency',
            packageName: 'duplicate',
          }),
        ])
      );
      expect(result.symbols).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            symbol: 'dead',
            verdict: 'transitive-dead',
            retainedBy: ['src/dead-user.ts'],
          }),
          expect.objectContaining({
            symbol: 'used',
            verdict: 'reachable',
          }),
        ])
      );
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  it('can return only the dynamic flow plan', async () => {
    const root = await fixture();
    try {
      const result = await analyzeResearchFlow({
        root,
        goal: 'how should an agent prove unused exports?',
        mode: 'plan',
      });
      expect(result.mode).toBe('plan');
      expect(result.summary.sourceFiles).toBe(0);
      expect(
        result.flow.some(step =>
          step.tools.includes('lspGetSemantics references')
        )
      ).toBe(true);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  it('surfaces heuristic research evidence as candidate, not proof', async () => {
    const root = await fixture();
    try {
      const result = await runOqlSearch({
        target: 'research',
        from: { kind: 'local', path: root },
        params: {
          goal: 'find unused exports and dependencies',
        },
      });
      expect('evidence' in result ? result.evidence.kind : undefined).toBe(
        'candidate'
      );
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});
