import { readdirSync, readFileSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = dirname(dirname(fileURLToPath(import.meta.url)));
function walk(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry =>
    entry.isDirectory()
      ? walk(join(directory, entry.name))
      : [join(directory, entry.name)]
  );
}
// The skill is self-contained guidance: Markdown only, naming package commands rather than
// implementation paths. Shared report assets belong to skills under skills/, not to this skill.
const skill = join(root, 'skills/octocode-chrome-devtools');
for (const file of walk(skill)) {
  const entry = relative(skill, file).replaceAll('\\', '/');
  if (!entry.endsWith('.md'))
    throw Error(`Chrome skill must contain guidance Markdown only: ${entry}`);
  if (
    /packages\/octocode-chrome-devtools|\b(?:src|dist)\//.test(
      readFileSync(file, 'utf8')
    )
  )
    throw Error(
      `Chrome skill must use package CLI/MCP, not implementation paths: ${entry}`
    );
}
console.log('Chrome guidance-only skill boundary passed');
