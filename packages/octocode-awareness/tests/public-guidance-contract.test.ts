import { readdirSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (path: string): string => readFileSync(path, 'utf8');
const markdownFiles = (root: string): string[] => readdirSync(root, { withFileTypes: true })
  .flatMap((entry) => entry.isDirectory()
    ? markdownFiles(resolve(root, entry.name))
    : entry.name.endsWith('.md') ? [resolve(root, entry.name)] : []);

describe('public Awareness guidance', () => {
  it('teaches canonical commands and separate Awareness storage', () => {
    const retiredProse = /\bRun `(setup|next|inspect|verify|close|init|refinement list)`/;
    const guides = [
      resolve(PACKAGE_ROOT, 'README.md'),
      resolve(PACKAGE_ROOT, 'AGENTS.md'),
      ...markdownFiles(resolve(PACKAGE_ROOT, 'docs')),
      ...markdownFiles(resolve(PACKAGE_ROOT, 'skills/octocode-awareness')),
    ].map(read).join('\n');
    for (const retired of [
      /octocode-awareness setup\b/,
      /octocode-awareness next\b/,
      /octocode-awareness inspect\b/,
      /octocode-awareness verify\s+--/,
      /octocode-awareness close\b/,
      /octocode-awareness init\b/,
      /octocode-awareness refinement list\b/,
      retiredProse,
    ]) expect(guides).not.toMatch(retired);
    expect('Run `init` once.').toMatch(retiredProse);
    expect('Run `refinement list`.').toMatch(retiredProse);
    expect(guides).not.toContain('.octocode/octocode.sqlite3');
    expect(guides).toContain('maintenance init');
    expect(guides).toContain('.octocode/awareness.sqlite3');
    expect(guides).toContain('OCTOCODE_HOME/awareness/awareness.sqlite3');
    expect(guides).toContain('agent/agent.sqlite3');
    expect(guides).toMatch(/Agent (?:control|databases?).{0,120}separate/is);
  });
});
