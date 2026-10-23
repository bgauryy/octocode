// Node side of the report viewer: find the workspace, collect Markdown, resolve code references.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { mdKit } from './markdown.mjs';

// The skill folder this renderer is installed in: <skill>/scripts/lib/report/ is four levels down.
export const SCRIPT_SKILL_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const SKIP_DIRS = new Set(['node_modules', 'tmp', 'view']);
export const PRIMARY = ['rfc.md', 'readme.md', 'index.md'];
export const KINDS = {
  'octocode-research': 'Research', 'octocode-rfc-generator': 'RFC', 'octocode-brainstorming': 'Brainstorm',
  'octocode-architect': 'Architecture', 'octocode-roast': 'Roast', 'octocode-clean-agentic-code': 'Cleanup',
  'octocode-documentation': 'Documentation', 'octocode-eval-benchmark': 'Evaluation', 'octocode-skills': 'Skill review',
  'octocode-agentic-prompts': 'Prompt review', 'octocode-scraping': 'Scrape', 'octocode-chrome-devtools': 'Browser session',
  'octocode-agents-communication': 'Coordination', 'octocode-workflows': 'Workflow',
};

export const toPosix = (p) => p.split(path.sep).join('/');

function findWorkspace(start) {
  let dir = path.resolve(start);
  if (fs.existsSync(dir) && !fs.statSync(dir).isDirectory()) dir = path.dirname(dir);
  for (let d = dir; ; d = path.dirname(d)) {
    if (path.basename(d) === '.octocode') return path.dirname(d);
    if (path.dirname(d) === d) break;
  }
  for (let d = dir; ; d = path.dirname(d)) {
    if (fs.existsSync(path.join(d, '.git'))) return d;
    if (path.dirname(d) === d) break;
  }
  return process.cwd();
}

function walk(dir, root, out) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, e.name);
    if (e.isDirectory()) {
      if (SKIP_DIRS.has(e.name) || e.name.startsWith('.')) continue;
      walk(full, root, out);
    } else if (e.isFile() && /\.(md|markdown)$/i.test(e.name)) out.push(full);
  }
  return out;
}

function sortKey(rel) {
  const parts = rel.split('/');
  const base = parts[parts.length - 1].toLowerCase();
  const primary = PRIMARY.indexOf(base);
  const date = (rel.match(/(\d{4}-\d{2}-\d{2})/) || [])[1] || '';
  return { depth: parts.length, primary: primary < 0 ? 9 : primary, date, rel };
}

function orderFiles(rels) {
  return rels.sort((a, b) => {
    const ka = sortKey(a), kb = sortKey(b);
    const ga = a.includes('/') ? a.split('/')[0] : '', gb = b.includes('/') ? b.split('/')[0] : '';
    if (ga !== gb) return ga.localeCompare(gb);
    if (ka.date !== kb.date) return kb.date.localeCompare(ka.date);
    if (ka.primary !== kb.primary) return ka.primary - kb.primary;
    if (ka.depth !== kb.depth) return ka.depth - kb.depth;
    return a.localeCompare(b);
  });
}

const CODE_REF = /^((?:\.{1,2}\/|\/|~\/)?[\w@.% +\-]+(?:\/[\w@.% +\-[\]()]+)*\/?)(?:[:#]L?(\d+)(?:[-–:]L?(\d+))?)?$/;

export function resolveRefs(body, docDir, workspace) {
  const refs = {};
  const candidates = new Set();
  const noFences = body.replace(/^ {0,3}(`{3,}|~{3,})[\s\S]*?^ {0,3}\1[ \t]*$/gm, '');
  for (const m of noFences.matchAll(/(`+)([^`\n]+?)\1/g)) candidates.add(m[2].trim());
  for (const m of noFences.matchAll(/\]\(\s*(?:<([^>\n]+)>|((?:[^()\s]|\([^()]*\))+))/g)) candidates.add(m[1] ?? m[2]);
  for (const m of noFences.matchAll(/^ {0,3}\[[^\]^][^\]]*\]:\s*(?:<([^>\n]+)>|(\S+))/gm)) candidates.add(m[1] ?? m[2]);
  for (const raw of candidates) {
    if (!raw || raw.length > 300 || /^[a-z][\w+.-]*:/i.test(raw) || raw.startsWith('#')) continue;
    const hash = raw.indexOf('#');
    const fragment = hash >= 0 && !/^L?\d+(?:[-–:]L?\d+)?$/.test(raw.slice(hash + 1)) ? raw.slice(hash + 1) : null;
    const m = (fragment === null ? raw : raw.slice(0, hash)).match(CODE_REF);
    if (!m || !/[./]/.test(m[1])) continue;
    let p = m[1];
    try { p = decodeURIComponent(p); } catch { /* keep raw */ }
    if (p.startsWith('~/')) p = path.join(os.homedir(), p.slice(2));
    const bases = path.isAbsolute(p) ? [''] : [docDir, workspace];
    for (const base of bases) {
      const abs = path.resolve(base, p);
      if (!fs.existsSync(abs)) continue;
      let line = m[2] ? Number(m[2]) : undefined;
      if (fragment) {
        if (!/\.(?:md|markdown|mdown)$/i.test(abs) || !fs.statSync(abs).isFile()) continue;
        let target;
        try { target = decodeURIComponent(fragment); } catch { target = fragment; }
        const sourceLines = fs.readFileSync(abs, 'utf8').split(/\r?\n/);
        const seen = {};
        let fence = null;
        for (let i = 0; i < sourceLines.length; i++) {
          const text = sourceLines[i];
          const marker = text.match(/^ {0,3}(`{3,}|~{3,})/);
          if (marker) {
            if (!fence) fence = marker[1];
            else if (marker[1][0] === fence[0] && marker[1].length >= fence.length) fence = null;
            continue;
          }
          if (fence) continue;
          const heading = text.match(/^ {0,3}#{1,6}\s+(.+?)\s*#*\s*$/)?.[1]
            || (text.trim() && /^ {0,3}(=+|-+)\s*$/.test(sourceLines[i + 1] || '') && !/^\s*[-*+|>]/.test(text) ? text.trim() : null);
          if (!heading) continue;
          const slug = mdKit().slugify(heading.replace(/\[([^\]]+)\]\([^)]*\)/g, '$1'));
          const count = seen[slug] || 0; seen[slug] = count + 1;
          if ((count ? `${slug}-${count}` : slug) === target) { line = i + 1; break; }
        }
        if (!line) continue;
      }
      const rel = toPosix(path.relative(workspace, abs)) || '.';
      refs[raw] = { abs: toPosix(abs), rel: rel.startsWith('..') ? toPosix(abs) : rel, ...(line ? { line } : {}) };
      break;
    }
  }
  return refs;
}

function kindFor(absFile) {
  const segs = toPosix(absFile).split('/');
  for (let i = segs.length - 2; i >= 0; i--) if (KINDS[segs[i]]) return KINDS[segs[i]];
  return '';
}

export function skillFor(absFile) {
  const segs = toPosix(absFile).split('/');
  for (let i = segs.length - 2; i >= 0; i--) if (KINDS[segs[i]]) return segs[i];
  return '';
}

export function collect(target, options = {}) {
  const abs = path.resolve(target);
  if (!fs.existsSync(abs)) throw new Error(`Not found: ${target}`);
  const isDir = fs.statSync(abs).isDirectory();
  const workspace = options.workspace ? path.resolve(options.workspace) : findWorkspace(abs);
  const root = isDir ? abs : path.dirname(abs);
  const list = isDir ? walk(abs, abs, []) : [abs];
  if (!list.length) throw new Error(`No Markdown files under ${target}`);
  const kit = mdKit();
  const rels = orderFiles(list.map((f) => toPosix(path.relative(root, f))));
  const underOctocode = path.basename(abs) === '.octocode';
  const files = rels.map((rel) => {
    const full = path.join(root, rel);
    const { meta, body } = kit.frontmatter(fs.readFileSync(full, 'utf8'));
    const first = rel.includes('/') ? rel.split('/')[0] : '';
    const kind = meta.kind || kindFor(full);
    const group = KINDS[first] || underOctocode ? first : (rel.includes('/') ? path.dirname(rel) : '');
    return { path: rel, group, kind, meta, body, source: toPosix(full), refs: resolveRefs(body, path.dirname(full), workspace) };
  });
  return { abs, isDir, workspace, root, rels, files, underOctocode, kit };
}
