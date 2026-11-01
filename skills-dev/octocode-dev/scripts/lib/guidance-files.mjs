// The Markdown an agent or developer reads about the Octocode tools: every skill in each collection, the repository
// docs, and each package README and docs folder. Packages that declare their own MCP tools through octocode-mcp-cli
// document their own vocabulary, so their docs are not prose about this tool contract.
import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { SKILL_COLLECTIONS } from './workspaces.mjs';

const SKIPPED_DIRS = new Set(['node_modules', 'dist', 'out', 'build', 'coverage', 'target', 'fixtures']);
const OWN_TOOLS_DEPENDENCY = 'octocode-mcp-cli';
// The canonical source of the shared references that skills/common/build.mjs copies into skills.
const SHARED_SOURCE = 'skills/common/';
const sourceFirst = (a, b) => Number(!a.startsWith(SHARED_SOURCE)) - Number(!b.startsWith(SHARED_SOURCE)) || a.localeCompare(b);

function markdownUnder(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
    if (entry.name.startsWith('.') || SKIPPED_DIRS.has(entry.name)) return [];
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return markdownUnder(path);
    return entry.isFile() && entry.name.endsWith('.md') ? [path] : [];
  });
}

function declaresOwnTools(packageDir) {
  const manifest = join(packageDir, 'package.json');
  if (!existsSync(manifest)) return false;
  const pkg = JSON.parse(readFileSync(manifest, 'utf8'));
  if (pkg.name === OWN_TOOLS_DEPENDENCY) return true;
  return [pkg.dependencies, pkg.devDependencies, pkg.peerDependencies].some(deps => deps && OWN_TOOLS_DEPENDENCY in deps);
}

function packageDocs(root) {
  const packages = join(root, 'packages');
  if (!existsSync(packages)) return [];
  return readdirSync(packages, { withFileTypes: true })
    .filter(entry => entry.isDirectory() && !declaresOwnTools(join(packages, entry.name)))
    .flatMap(entry => {
      const dir = join(packages, entry.name);
      return [join(dir, 'README.md'), ...markdownUnder(join(dir, 'docs'))].filter(existsSync);
    });
}

/**
 * Root-relative guidance files. A byte-identical copy (a shared reference built into several skills) is listed once,
 * under its `skills/common/` source when there is one, else under the first path in sort order.
 */
export function guidanceFiles(root) {
  const all = [
    ...SKILL_COLLECTIONS.flatMap(collection => markdownUnder(join(root, collection))),
    ...markdownUnder(join(root, 'docs')),
    ...packageDocs(root),
  ].map(path => relative(root, path).replaceAll('\\', '/')).toSorted(sourceFirst);
  const seen = new Set();
  return all.filter(path => {
    const digest = createHash('sha256').update(readFileSync(join(root, path))).digest('hex');
    if (seen.has(digest)) return false;
    seen.add(digest);
    return true;
  });
}
