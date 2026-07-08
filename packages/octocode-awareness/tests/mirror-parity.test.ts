import { describe, expect, it } from 'vitest';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { dirname } from 'node:path';

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = resolve(TEST_DIR, '..');
const REPO_ROOT = resolve(PACKAGE_ROOT, '../..');

const PACKAGE_SKILLS = [
  'octocode-awareness',
  'octocode-skills',
] as const;

const LOCAL_AGENT_MIRROR_ROOT = resolve(REPO_ROOT, '.agents/skills');
const MIRROR_ROOTS = [
  ...(existsSync(LOCAL_AGENT_MIRROR_ROOT) ? [LOCAL_AGENT_MIRROR_ROOT] : []),
] as const;

function listFiles(root: string): string[] {
  const out: string[] = [];
  function walk(dir: string): void {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (entry.name === 'node_modules' || entry.name === '.DS_Store') continue;
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

describe('package-owned skill mirrors', () => {
  for (const skill of PACKAGE_SKILLS) {
    for (const mirrorRoot of MIRROR_ROOTS) {
      it(`${skill} matches ${relative(REPO_ROOT, mirrorRoot)}`, () => {
        const source = resolve(PACKAGE_ROOT, 'skills', skill);
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
