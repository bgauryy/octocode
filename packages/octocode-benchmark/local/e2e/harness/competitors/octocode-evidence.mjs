// Line/file evidence from octocode structured results (continuations excluded),
// shape-agnostic across the local and GitHub tools, for the shared truth check.
import { inventoryRows, outlineRows, structureFiles } from '../responses.mjs';
import { isHintKey } from '../sensors.mjs';

const pair = (ev, file, line) => { if (file && Number.isInteger(line)) ev.pairs.push({ file, line }); };
const listFile = (ev, file) => { ev.files.add(file); ev.fileRows.push(file); };
/** `N<TAB>text` lines of a string as line pairs of `file`. */
const numbered = (ev, file, content) => { for (const m of String(content).matchAll(/^(\d+)\t/gm)) pair(ev, file, +m[1]); };

/** Line citations a node carries for `here` (its own path or the nearest ancestor's). */
function nodeLines(ev, node, here) {
  if (node.displayRange && typeof node.path === 'string') pair(ev, node.path, node.displayRange.startLine);
  // lspSearch callers: the row `line` is the caller's declaration; `sites` are the call lines.
  if (Array.isArray(node.sites)) { for (const site of node.sites) pair(ev, here, site?.line); }
  else if (typeof node.line === 'number') pair(ev, here, node.line);
  if (typeof node.content === 'string') numbered(ev, here, node.content);
  if (Array.isArray(node.lines) && node.lines.every(l => typeof l === 'string')) {
    for (const l of node.lines) { const m = /^(\d+)\t/.exec(l); if (m) pair(ev, here, +m[1]); }
  }
}

/**
 * Compact rows: symbols outline rows (grouped and merged, see outlineRows) and
 * lean structural matches "<line>[-<end>]\t<value>". Other tools' `symbols`
 * arrays may hold anything: only declaration objects are outline rows.
 */
function compactRows(ev, node, here) {
  if (Array.isArray(node.symbols)) {
    for (const decl of outlineRows(node.symbols.filter(row => row && typeof row === 'object' && !Array.isArray(row)))) pair(ev, here, decl.line);
  }
  if (!Array.isArray(node.matches)) return;
  for (const row of node.matches.filter(r => typeof r === 'string')) {
    // Grouped lspSearch callers: "<line>:<col>[,<line>:<col>…] in <kind> <name> …" lists call sites.
    const sites = /^([\d:,]+) in /.exec(row)?.[1].split(',');
    if (sites) for (const site of sites) pair(ev, here, +site.split(':')[0]);
    else { const m = /^\s*(\d+)/.exec(row); if (m) pair(ev, here, +m[1]); }
  }
}

/** Listed files of a structure row (structureSearch `path` + `files`, ghStructure `dir` + `files`). */
function listedFiles(ev, tool, node) {
  if (!Array.isArray(node.files)) return;
  if (tool === 'structureSearch') {
    // structureSearch `files`: bare entries are `path`'s own, group dirs are relative to `path`;
    // groups (`dir` rows) were collected with their row.
    if (typeof node.path === 'string') for (const f of structureFiles(node.files, node.path)) listFile(ev, f.path);
    return;
  }
  if (typeof node.dir !== 'string') return;
  // ghStructure `dir` is the repo path; files read "<name> (<bytes>[, <YYYY-MM-DD>])" (GS3).
  const rel = node.dir === '.' || node.dir === '' ? '' : node.dir.replace(/\/$/, '');
  const dir = rel ? `${rel}/` : '';
  for (const name of node.files.filter(f => typeof f === 'string')) {
    listFile(ev, dir + (tool === 'ghStructure' ? name.replace(/ \(\d+(?:, \d{4}-\d{2}-\d{2})?\)$/, '') : name));
  }
}

/** A child the walk handles itself (true) or skips; false = walk into it. */
function specialChild(c, key, child) {
  if (isHintKey(key)) return true;
  if (key === 'unsearchedFiles' && Array.isArray(child)) {
    if (c.unsearched) for (const s of child) { const m = /^!\w+ (.+)$/.exec(s); if (m) c.ev.files.add(m[1]); }
    return true;
  }
  if (key === 'files' && c.tool === 'ghGetHistoryItem' && Array.isArray(child) && child.some(f => typeof f === 'string' || (f && !('path' in f)))) {
    for (const f of inventoryRows(child)) if (f.path) c.ev.files.add(f.path);
    return true;
  }
  // `enclosing` names the declaration around a hit; its line is not a hit.
  return key === 'from' || key === 'displayRange' || key === 'enclosing';
}

function walk(c, node, parentPath, inArray) {
  if (!node || typeof node !== 'object') return;
  if (Array.isArray(node)) { for (const item of node) walk(c, item, parentPath, true); return; }
  const { ev } = c;
  const here = typeof node.path === 'string' ? node.path : parentPath;
  if (typeof node.path === 'string' && inArray) listFile(ev, node.path);
  if (typeof node.file === 'string') ev.files.add(node.file);
  if (node.from?.path && Array.isArray(node.fromRanges)) { for (const r of node.fromRanges) pair(ev, node.from.path, r.startLine); return; }
  nodeLines(ev, node, here);
  compactRows(ev, node, here);
  listedFiles(ev, c.tool, node);
  for (const [key, child] of Object.entries(node)) if (!specialChild(c, key, child)) walk(c, child, here, Array.isArray(child));
}

function leaves(value, out = []) {
  if (value == null) return out;
  if (typeof value !== 'object') { out.push(String(value)); return out; }
  for (const child of Object.values(value)) leaves(child, out);
  return out;
}

/** Add one octocode response's evidence to `ev`; `unsearched` counts `unsearchedFiles` as listed files. */
export function octocodeEvidence(entry, ev, { unsearched = false } = {}) {
  walk({ ev, tool: entry.tool, unsearched }, entry.sc, undefined, false);
  ev.text += `${leaves(entry.sc ?? entry.text).join('\n')}\n`;
}
