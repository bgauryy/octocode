import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
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
const skill = join(root, '../../skills/octocode-chrome-devtools');
const common = join(root, '../../skills/common');
const guidance = new Set(['SKILL.md', 'README.md', 'output.md']);
const reportScripts = new Set([
  'scripts/render-report.mjs',
  'scripts/lib/entrypoint.mjs',
]);
for (const file of walk(skill)) {
  const entry = relative(skill, file).replaceAll('\\', '/');
  if (guidance.has(entry)) continue;
  const source = join(common, entry);
  const shared =
    reportScripts.has(entry) ||
    /^scripts\/lib\/report\/[^/]+\.mjs$/.test(entry) ||
    /^references\/[^/]+\.md$/.test(entry);
  if (
    !shared ||
    !existsSync(source) ||
    !readFileSync(file).equals(readFileSync(source))
  )
    throw Error(
      `Chrome skill must contain guidance and canonical shared report assets only: ${entry}`
    );
}
if (
  /packages\/octocode-chrome-devtools|\b(?:src|dist)\//.test(
    readFileSync(join(skill, 'SKILL.md'), 'utf8')
  )
)
  throw Error(
    'Chrome skill must use package CLI/MCP, not implementation paths'
  );
console.log('Chrome syntax and guidance-only skill boundary passed');
