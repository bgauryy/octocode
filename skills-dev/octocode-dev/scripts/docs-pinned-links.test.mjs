import assert from 'node:assert/strict';
import test from 'node:test';
import { ROOT } from './lib/workspaces.mjs';
import { gitHasPath, pinnedLinkFailures } from './docs-pinned-links.mjs';

const pin = (sha, file) => `https://github.com/bgauryy/octocode/blob/${sha}/${file}`;

test('a pin naming a commit or path that does not exist fails', () => {
  const missingSha = 'deadbeef'.repeat(5);
  const failures = pinnedLinkFailures('doc.md', [pin(missingSha, 'docs/OCTOCODE_TOOLS.md')], (sha, file) => gitHasPath(ROOT, sha, file));
  assert.equal(failures.length, 1);
  assert.match(failures[0], /doc\.md pins "docs\/OCTOCODE_TOOLS\.md" at commit deadbeef/);
});

test('a pin to an existing commit and path passes, and a missing path at it fails', () => {
  const head = 'HEAD';
  const has = (sha, file) => gitHasPath(ROOT, sha, file);
  assert.deepEqual(pinnedLinkFailures('doc.md', [pin(head, 'x')], has), [], 'HEAD is not a hex sha: not a pin');
  const sha = pinnedSha();
  assert.deepEqual(pinnedLinkFailures('doc.md', [pin(sha, 'package.json')], has), []);
  assert.equal(pinnedLinkFailures('doc.md', [pin(sha, 'no/such/file.md')], has).length, 1);
});

test('links to other repositories and blob/main links are not pins', () => {
  const never = () => false;
  assert.deepEqual(
    pinnedLinkFailures('doc.md', ['https://github.com/microsoft/vscode/blob/da36e0eba18babc8eb3e18f5a9b30e1dd7e083d4/a.ts', 'https://github.com/bgauryy/octocode/blob/main/README.md'], never),
    []
  );
});

function pinnedSha() {
  return execGit(['rev-parse', 'HEAD']);
}

import { execFileSync } from 'node:child_process';
function execGit(args) {
  return execFileSync('git', args, { cwd: ROOT, encoding: 'utf8' }).trim();
}
