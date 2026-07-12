import { describe, expect, it } from 'vitest';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { dirname } from 'node:path';

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = resolve(TEST_DIR, '..');
const REPO_ROOT = resolve(PACKAGE_ROOT, '../..');

// Retired names must stay in sync with build.mjs's retiredPackageSkills —
// duplicated here (not imported) so this test still fails loudly if build.mjs
// ever drops a skill without also retiring it, or vice versa.
const RETIRED_PACKAGE_SKILLS = ['octocode-agent-communication', 'octocode-reflection'];

// Discovered from repo skills/ (not hardcoded) so every skill build.mjs
// bundles is automatically covered by this parity test — adding a new skill
// folder requires no test edits.
function discoverPackageSkills(): string[] {
  const skillsRoot = resolve(REPO_ROOT, 'skills');
  return readdirSync(skillsRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((name) => name !== 'scripts' && !RETIRED_PACKAGE_SKILLS.includes(name))
    .filter((name) => existsSync(resolve(skillsRoot, name, 'SKILL.md')))
    .sort();
}

const PACKAGE_SKILLS = discoverPackageSkills();

const LOCAL_AGENT_MIRROR_ROOT = resolve(REPO_ROOT, '.agents/skills');
const MIRROR_ROOTS = [
  ...(existsSync(LOCAL_AGENT_MIRROR_ROOT) ? [LOCAL_AGENT_MIRROR_ROOT] : []),
] as const;

// The build intentionally excludes this gitignored, machine-generated file
// from every mirror/bundle (see build.mjs's skipGeneratedConfig), so it must
// also be excluded here or a real source-tree copy would fail parity.
function listFiles(root: string): string[] {
  const out: string[] = [];
  function walk(dir: string): void {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (entry.name === 'node_modules' || entry.name === '.DS_Store' || entry.name === 'octocode-config.mjs') continue;
      const abs = resolve(dir, entry.name);
      if (entry.isDirectory()) {
        walk(abs);
      } else if (entry.isFile()) {
        out.push(relative(root, abs));
      }
    }
  }
  walk(root);
  return out.sort();
}

describe('package-bundled skill mirrors', () => {
  it('discovers at least the required skills', () => {
    expect(PACKAGE_SKILLS).toContain('octocode-awareness');
    expect(PACKAGE_SKILLS).toContain('octocode-research');
    expect(PACKAGE_SKILLS).not.toContain('octocode-skills');
  });

  for (const skill of PACKAGE_SKILLS) {
    for (const mirrorRoot of MIRROR_ROOTS) {
      it(`${skill} matches ${relative(REPO_ROOT, mirrorRoot)}`, () => {
        const source = resolve(REPO_ROOT, 'skills', skill);
        const mirror = resolve(mirrorRoot, skill);
        expect(existsSync(mirror), `${mirror} should exist after awareness build`).toBe(true);
        expect(statSync(mirror).isDirectory()).toBe(true);

        const sourceFiles = listFiles(source);
        const mirrorFiles = listFiles(mirror);
        expect(mirrorFiles).toEqual(sourceFiles);

        for (const file of sourceFiles) {
          expect(readFileSync(resolve(mirror, file), 'utf8')).toBe(readFileSync(resolve(source, file), 'utf8'));
        }
      });
    }
  }
});
