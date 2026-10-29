// How a graph target is named in source: its file stem, or for a Rust crate root the crate name
// (`crates/contract-types/src/lib.rs` is written `octocode_contract_types::X`). Pure, cached per crate dir.
import { existsSync, readFileSync } from 'node:fs';
import { basename, dirname, extname, join } from 'node:path';

const ROOTS = ['index', 'mod', 'lib', 'main', '__init__'];
// A root file is named by its directory; a `src` directory names nothing, so `crates/x/src/lib.rs` is `x`.
export const stemOf = (id) => {
  const b = basename(id, extname(id));
  if (!ROOTS.includes(b) || dirname(id) === '.') return b;
  let dir = dirname(id);
  if (basename(dir) === 'src' && dirname(dir) !== '.') dir = dirname(dir);
  return basename(dir);
};

const crates = new Map();
// The `[package] name` of the nearest Cargo.toml above `abs`, in path form (`-` becomes `_`); null when none.
export function crateNameOf(abs) {
  let dir = dirname(abs);
  const seen = [];
  while (dir && dir !== dirname(dir)) {
    if (crates.has(dir)) { const v = crates.get(dir); for (const d of seen) crates.set(d, v); return v; }
    seen.push(dir);
    const manifest = join(dir, 'Cargo.toml');
    if (existsSync(manifest)) {
      let name = null;
      let section = '';
      for (const line of readFileSync(manifest, 'utf8').split('\n')) {
        const header = line.match(/^\s*\[([^\]]+)\]/);
        if (header) { section = header[1].trim(); continue; }
        const m = section === 'package' && line.match(/^\s*name\s*=\s*"([^"]+)"/);
        if (m) { name = m[1]; break; }
      }
      const v = name ? name.replace(/-/g, '_') : null;
      for (const d of seen) crates.set(d, v);
      return v;
    }
    dir = dirname(dir);
  }
  for (const d of seen) crates.set(d, null);
  return null;
}

export const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
// The name in `stems` that `text` actually uses, else the first.
export const usedName = (stems, text) => stems.find((s) => new RegExp(`\\b${escapeRe(s)}\\b`).test(text)) ?? stems[0];

// Every name an import of `id` (section-relative) may use; `abs` resolves the section root for Cargo.toml lookups.
export function stemsOf(id, abs) {
  const stems = [stemOf(id)];
  if (extname(id) === '.rs') {
    const snake = stems[0].replace(/-/g, '_');
    if (snake !== stems[0]) stems.push(snake);
    if (['lib', 'main'].includes(basename(id, '.rs')) && abs) {
      const crate = crateNameOf(abs);
      if (crate && !stems.includes(crate)) stems.push(crate);
    }
  }
  return stems;
}
