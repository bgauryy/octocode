// Verify relative Markdown links in this package: the target exists, is not
// gitignored (ignored files are never in a clone), and any #anchor names a heading.
// Usage: node src/check-links.mjs [extra.md ...]
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const skipDirs = new Set(['node_modules', 'target', 'out', 'bin', '.git']);

function markdownFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return skipDirs.has(entry.name) ? [] : markdownFiles(path);
    return entry.name.endsWith('.md') ? [path] : [];
  });
}

// Strip fenced blocks and inline code so example syntax is not treated as a link.
function prose(text) {
  return text.replace(/^(```|~~~)[^\n]*\n[\s\S]*?^\1[^\n]*$/gm, m => m.replace(/[^\n]/g, ' '))
    .replace(/`[^`\n]*`/g, m => ' '.repeat(m.length));
}

// GitHub heading anchors: lowercase, drop punctuation, spaces to hyphens, -N for repeats.
function anchors(file) {
  const seen = new Map(), result = new Set();
  for (const [, heading] of prose(readFileSync(file, 'utf8')).matchAll(/^#{1,6}\s+(.+?)\s*#*\s*$/gm)) {
    const base = heading.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1').toLowerCase().trim()
      .replace(/[^\p{L}\p{N}\s_-]/gu, '').replace(/\s/g, '-');
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    result.add(count ? `${base}-${count}` : base);
  }
  return result;
}

function ignored(paths) {
  if (!paths.length) return new Set();
  try {
    const output = execFileSync('git', ['check-ignore', '--no-index', '--stdin'], { cwd: root, input: paths.join('\n'), encoding: 'utf8' });
    return new Set(output.split('\n').filter(Boolean));
  } catch (error) {
    if (error.status === 1) return new Set(); // nothing ignored
    throw error;
  }
}

const files = [...markdownFiles(root), ...process.argv.slice(2).map(p => resolve(p))];
const links = [];
for (const file of files) {
  const text = prose(readFileSync(file, 'utf8'));
  const lines = text.split('\n');
  lines.forEach((line, index) => {
    for (const match of line.matchAll(/\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)|^\s*\[[^\]]+\]:\s+(\S+)/g)) {
      const href = match[1] ?? match[2];
      if (/^[a-z][a-z0-9+.-]*:/i.test(href)) continue; // external or mailto
      const [pathPart, anchor] = href.split('#');
      const target = pathPart ? resolve(dirname(file), decodeURIComponent(pathPart)) : file;
      links.push({ file, line: index + 1, href, target, anchor });
    }
  });
}

const problems = [];
const where = link => `${relative(process.cwd(), link.file)}:${link.line}: ${link.href}`;
const existing = links.filter(link => {
  if (existsSync(link.target)) return true;
  problems.push(`${where(link)}: target does not exist`);
  return false;
});
const ignoredTargets = ignored([...new Set(existing.map(link => link.target))]);
const anchorCache = new Map();
for (const link of existing) {
  if (ignoredTargets.has(link.target)) { problems.push(`${where(link)}: target is gitignored`); continue; }
  if (!link.anchor || !link.target.endsWith('.md') || statSync(link.target).isDirectory()) continue;
  if (!anchorCache.has(link.target)) anchorCache.set(link.target, anchors(link.target));
  if (!anchorCache.get(link.target).has(link.anchor.toLowerCase())) problems.push(`${where(link)}: missing heading anchor`);
}

if (problems.length) {
  console.error(problems.join('\n'));
  console.error(`${problems.length} broken link(s) in ${files.length} Markdown files`);
  process.exit(1);
}
console.log(`${links.length} relative links in ${files.length} Markdown files resolve`);
