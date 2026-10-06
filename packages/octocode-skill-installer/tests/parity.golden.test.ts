/**
 * Golden-output parity suite for the skill installer
 * (RFC post-audit-hardening-2026-09, S6).
 *
 * Each scenario is fully self-describing: the fixture records the setup
 * (pre-existing trees), the install options for each run, the returned
 * result object, and the resulting filesystem tree — all with the temp root
 * normalized to "<ROOT>".
 *
 * Fixture format (paths normalized: temp root → "<ROOT>", "/" separators):
 *   {
 *     "setup": [{ "dir": "<ROOT>/...", "marker": "v1" }, ...],
 *     "runs":  [<InstallBundledSkillsOptions, normalized>, ...],
 *     "result": <last-run result or array of results, normalized>,
 *     "tree":   [{ "path": "...", "kind": "dir"|"file"|"symlink",
 *                  "target"?: "...", "content"?: "..." }, ...]  // sorted
 *   }
 * A setup entry materializes: <dir>/SKILL.md = "# Fixture skill (<marker>)\n"
 * and <dir>/references/guide.md = "<marker> guide\n".
 *
 * Regenerate after an intentional behavior change:
 *   UPDATE_SKILL_PARITY_GOLDENS=1 yarn vitest run tests/parity.golden.test.ts
 */

import {
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  readlinkSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import {
  installBundledSkills,
  type InstallBundledSkillsOptions,
} from '../src/index.js';

const FIXTURE_DIR = resolve(import.meta.dirname, 'fixtures', 'parity');
const UPDATE = process.env['UPDATE_SKILL_PARITY_GOLDENS'] === '1';

const roots: string[] = [];
afterEach(() => {
  while (roots.length > 0) {
    rmSync(roots.pop()!, { recursive: true, force: true });
  }
});

function tempRoot(): string {
  const root = mkdtempSync(join(tmpdir(), 'skill-parity-'));
  roots.push(root);
  return root;
}

interface SetupEntry {
  dir: string;
  marker: string;
}

function materialize(root: string, setup: SetupEntry[]): void {
  for (const entry of setup) {
    const dir = entry.dir.split('<ROOT>').join(root);
    mkdirSync(join(dir, 'references'), { recursive: true });
    writeFileSync(join(dir, 'SKILL.md'), `# Fixture skill (${entry.marker})\n`);
    writeFileSync(join(dir, 'references', 'guide.md'), `${entry.marker} guide\n`);
  }
}

function normalize(value: unknown, root: string): unknown {
  return JSON.parse(
    JSON.stringify(value).split(root.split('\\').join('/')).join('<ROOT>')
  );
}

function denormalizeOptions(
  options: unknown,
  root: string
): InstallBundledSkillsOptions {
  return JSON.parse(
    JSON.stringify(options).split('<ROOT>').join(root.split('\\').join('/'))
  ) as InstallBundledSkillsOptions;
}

interface TreeEntry {
  path: string;
  kind: 'dir' | 'file' | 'symlink';
  target?: string;
  content?: string;
}

function snapshotTree(root: string): TreeEntry[] {
  const entries: TreeEntry[] = [];
  const walk = (dir: string) => {
    for (const name of readdirSync(dir).sort()) {
      const p = join(dir, name);
      const rel = p.slice(root.length + 1).split('\\').join('/');
      const stat = lstatSync(p);
      if (stat.isSymbolicLink()) {
        entries.push({
          path: rel,
          kind: 'symlink',
          target: readlinkSync(p).split(root).join('<ROOT>'),
        });
      } else if (stat.isDirectory()) {
        entries.push({ path: rel, kind: 'dir' });
        walk(p);
      } else {
        entries.push({
          path: rel,
          kind: 'file',
          content: readFileSync(p, 'utf8'),
        });
      }
    }
  };
  walk(root);
  // Byte order, not localeCompare: deterministic across ICU builds and
  // identical to the native (Rust) parity walk.
  return entries.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
}

function runScenario(
  name: string,
  setup: SetupEntry[],
  runs: InstallBundledSkillsOptions[]
): void {
  it(`parity: ${name}`, () => {
    const root = tempRoot();
    materialize(root, setup);
    // Later option sets model repeat runs (idempotence, upgrade, conflict).
    const normalizedRuns = normalize(runs, root);
    const results = (normalizedRuns as unknown[]).map(options =>
      installBundledSkills(denormalizeOptions(options, root))
    );
    const golden = {
      setup,
      runs: normalizedRuns,
      result: normalize(results.length === 1 ? results[0] : results, root),
      tree: snapshotTree(root),
    };
    const fixturePath = join(FIXTURE_DIR, `${name}.json`);
    if (UPDATE || !existsSync(fixturePath)) {
      mkdirSync(dirname(fixturePath), { recursive: true });
      writeFileSync(fixturePath, `${JSON.stringify(golden, null, 2)}\n`);
    }
    const expected = JSON.parse(readFileSync(fixturePath, 'utf8')) as unknown;
    expect(golden).toEqual(expected);
  });
}

const BUNDLED = '<ROOT>/bundled/fixture-skill';
const CANONICAL_DIR = '<ROOT>/home/.octocode/skills';
const CANONICAL = `${CANONICAL_DIR}/fixture-skill`;
const CLAUDE_DEST = '<ROOT>/home/.claude/skills/fixture-skill';

function baseOptions(
  overrides: Partial<InstallBundledSkillsOptions> = {}
): InstallBundledSkillsOptions {
  return {
    skills: [{ name: 'fixture-skill', sourcePath: BUNDLED }],
    canonicalSkillsDir: CANONICAL_DIR,
    targets: [
      { platform: 'claude', scope: 'global', homeDir: '<ROOT>/home' },
      { platform: 'cursor', scope: 'global', homeDir: '<ROOT>/home' },
      { platform: 'codex', scope: 'project', projectDir: '<ROOT>/project' },
    ],
    mode: 'symlink',
    ...overrides,
  };
}

describe('skill installer golden parity (S6)', () => {
  runScenario(
    'fresh-install-symlink',
    [{ dir: BUNDLED, marker: 'v1' }],
    [baseOptions()]
  );

  runScenario(
    'fresh-install-copy',
    [{ dir: BUNDLED, marker: 'v1' }],
    [baseOptions({ mode: 'copy' })]
  );

  runScenario(
    'auto-mode-uses-platform-defaults',
    [{ dir: BUNDLED, marker: 'v1' }],
    [baseOptions({ mode: 'auto' })]
  );

  runScenario(
    'reinstall-is-idempotent',
    [{ dir: BUNDLED, marker: 'v1' }],
    [baseOptions(), baseOptions()]
  );

  runScenario(
    'conflict-without-force',
    [
      { dir: BUNDLED, marker: 'v1' },
      { dir: CANONICAL, marker: 'stale' },
      { dir: CLAUDE_DEST, marker: 'foreign' },
    ],
    [baseOptions()]
  );

  runScenario(
    'force-overwrites-conflicts',
    [
      { dir: BUNDLED, marker: 'v1' },
      { dir: CANONICAL, marker: 'stale' },
      { dir: CLAUDE_DEST, marker: 'foreign' },
    ],
    [baseOptions({ force: true })]
  );

  runScenario(
    'upgrade-refreshes-canonical-and-managed-copies',
    [
      { dir: BUNDLED, marker: 'v2' },
      { dir: CANONICAL, marker: 'v1' },
      { dir: CLAUDE_DEST, marker: 'v1' },
    ],
    [baseOptions({ mode: 'copy', upgrade: true })]
  );

  runScenario(
    'dry-run-reports-without-writing',
    [{ dir: BUNDLED, marker: 'v1' }],
    [baseOptions({ dryRun: true })]
  );

  runScenario(
    'invalid-name-fails-closed',
    [{ dir: BUNDLED, marker: 'v1' }],
    [
      baseOptions({
        skills: [{ name: '../escape', sourcePath: BUNDLED }],
      }),
    ]
  );
});
