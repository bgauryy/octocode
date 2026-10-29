// Where an imported name is used: the importing file, plus Rust child modules that inherit its imports with `use super::*`.
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { basename, dirname, extname, join } from 'node:path';

const re = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const mentions = (text, name) => new RegExp(`\\b${re(name)}\\b`).test(text);
const callLike = (text, name) => new RegExp(`\\b${re(name)}\\s*(\\(|::|\\.|!)`).test(text);

// Count lines that use any of `names`, skipping the import statement's own lines [from, to] (1-based).
export function countUsage(lines, names, from = 0, to = 0) {
  let usage = 0, calls = 0;
  lines.forEach((text, i) => {
    if (i + 1 >= from && i + 1 <= to) return;
    const hit = names.filter((n) => mentions(text, n));
    if (!hit.length) return;
    usage++;
    if (hit.some((n) => callLike(text, n))) calls++;
  });
  return { usage, calls };
}

function rsFiles(dir) {
  const out = [];
  for (const d of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, d.name);
    if (d.isDirectory()) out.push(...rsFiles(p));
    else if (d.name.endsWith('.rs')) out.push(p);
  }
  return out;
}

// Child modules of foo.rs live in foo/; a glob `use super::*` there reuses foo.rs imports. mod.rs/lib.rs/main.rs have no private child folder.
export function childUsage(absFile, names) {
  if (extname(absFile) !== '.rs' || ['mod', 'lib', 'main'].includes(basename(absFile, '.rs'))) return { usage: 0, calls: 0, files: [] };
  const dir = join(dirname(absFile), basename(absFile, '.rs'));
  if (!existsSync(dir) || !statSync(dir).isDirectory()) return { usage: 0, calls: 0, files: [] };
  let usage = 0, calls = 0;
  const files = [];
  for (const file of rsFiles(dir)) {
    const text = readFileSync(file, 'utf8');
    const inherits = /^\s*use\s+super::\*\s*;/m.test(text) || names.some((n) => new RegExp(`use\\s+super::(?:\\{[^}]*\\b${re(n)}\\b|${re(n)}\\b)`).test(text));
    if (!inherits) continue;
    const r = countUsage(text.split('\n'), names);
    if (r.usage) { usage += r.usage; calls += r.calls; files.push(basename(file)); }
  }
  return { usage, calls, files };
}
