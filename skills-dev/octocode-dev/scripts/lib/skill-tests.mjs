// Find tests owned by standalone skills. Workspace-owned tests have their own gates.
import fs from 'node:fs';
import path from 'node:path';

const ROOTS = ['skills', 'skills-beta', 'skills-dev'];
const WORKSPACE_TEST_OWNERS = new Map([
  ['skills', new Set(['common'])],
  ['skills-dev', new Set(['octocode-dev'])],
]);

export function discoverSkillTests(root) {
  const tests = [];

  function visit(relativeDirectory) {
    for (const entry of fs.readdirSync(path.join(root, relativeDirectory), { withFileTypes: true })) {
      const relativePath = path.join(relativeDirectory, entry.name);
      if (entry.isDirectory()) {
        if (entry.name.startsWith('.') || entry.name === 'node_modules' || WORKSPACE_TEST_OWNERS.get(relativeDirectory)?.has(entry.name)) continue;
        visit(relativePath);
      } else if (entry.isFile() && entry.name.endsWith('.test.mjs')) {
        tests.push(relativePath);
      }
    }
  }

  for (const directory of ROOTS) {
    if (fs.existsSync(path.join(root, directory))) visit(directory);
  }
  return tests.toSorted((a, b) => a.localeCompare(b));
}

export function skillTestCommand(root) {
  const tests = discoverSkillTests(root);
  if (tests.length === 0) throw new Error('No standalone skill tests found; refusing bare node --test discovery.');
  return ['node', ['--test', ...tests]];
}
