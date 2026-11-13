import { readdirSync, readFileSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { sharedAssets } from '../../../skills/common/build.mjs';
const root = dirname(dirname(fileURLToPath(import.meta.url)));
function walk(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry =>
    entry.isDirectory()
      ? walk(join(directory, entry.name))
      : [join(directory, entry.name)]
  );
}
for (const directory of ['tools', 'bin', 'tests'])
  for (const file of walk(join(root, directory)).filter(file =>
    file.endsWith('.mjs')
  )) {
    const result = spawnSync(process.execPath, ['--check', file], {
      encoding: 'utf8',
    });
    if (result.status !== 0) throw Error(result.stderr);
  }
// The skill holds guidance only: shared assets byte-identical to their canonical source, and
// its own Markdown, which names package commands rather than implementation paths.
const skill = join(root, '../../skills/octocode-chrome-devtools');
const canonical = sharedAssets(skill);
for (const file of walk(skill)) {
  const entry = relative(skill, file).replaceAll('\\', '/');
  const expected = canonical.get(entry);
  if (expected) {
    if (!readFileSync(file).equals(expected))
      throw Error(
        `Chrome skill shared asset differs from its canonical source: ${entry}`
      );
    continue;
  }
  if (!entry.endsWith('.md'))
    throw Error(
      `Chrome skill must contain guidance and canonical shared assets only: ${entry}`
    );
  if (
    /packages\/octocode-chrome-devtools|\b(?:src|dist)\//.test(
      readFileSync(file, 'utf8')
    )
  )
    throw Error(
      `Chrome skill must use package CLI/MCP, not implementation paths: ${entry}`
    );
}
console.log('Chrome syntax and guidance-only skill boundary passed');
