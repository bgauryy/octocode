#!/usr/bin/env node
/**
 * Mechanical checks for Markdown docs: local links and anchors, links that still point at a renamed file or
 * heading, heading structure, terms from the bundled Google word list, the style guide's deterministic rules,
 * and commands that run a local script or package script. Whether a hit is wrong in context stays a judgment.
 */
import { existsSync, readdirSync, readFileSync, realpathSync, statSync, writeFileSync } from 'node:fs';
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { errorMessage, fail } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

export const WORD_LIST = fileURLToPath(new URL('../assets/google-word-list.tsv', import.meta.url));
export const CHECKS = ['links', 'anchors', 'headings', 'words', 'style', 'commands'];
export const VERDICTS = ['dont-use', 'avoid', 'caution', 'usage'];
const DEFAULT_VERDICTS = ['dont-use', 'avoid'];
const MARKDOWN = /\.(?:md|mdx|markdown)$/i;
const EXTERNAL = /^(?:[a-z][a-z\d+.-]*:|\/\/)/i;
const TEMPLATED_LINK = /[<>{}*…$]/;
const TEMPLATED_COMMAND = /[<>{}*…$~]/;
const JOIN = String.raw`[\p{L}\p{M}\p{N}_/@-]`;
const MARK = '';

const USAGE = `Usage: node doc-check.mjs <file.md|folder>... [--renamed <old>=<new>]... [--write] [--root <dir>]
                         [--checks <list>] [--verdicts <list>]
       node doc-check.mjs --lookup <word>...

Checks Markdown files from any working directory. A folder adds every .md, .mdx and .markdown file below it
(dot-folders and node_modules skipped). Front matter, fenced code, inline code and HTML comments are never read
as links or prose.

--checks <list>    Comma-separated subset of ${CHECKS.join(',')} (default: all). Each check fills one list:
  links            brokenLinks: a local link, image or link definition whose target is missing or differs in
                   letter case (reason missing, case-mismatch or empty-target), or a [text][label] whose label
                   has no definition (undefined-reference)
  anchors          missingAnchors: a #fragment with no GitHub-style heading slug or id in its Markdown target;
                   suggestion names the anchor the fragment was probably meant to be, when one exists
  headings         headingIssues: missing-h1, multiple-h1, skipped-level, end-punctuation, sentence-case
                   (a capitalized word inside a heading that no scanned file capitalizes mid-sentence)
  words            wordListHits: terms of assets/google-word-list.tsv in prose, skipping words a style rule
                   already reports; wordGuidance maps each hit term to its guidance
  style            styleIssues: word-choice (with its replacement), filler, should-may, curly-quote,
                   date-format, time-format, vague-link-text, placeholder-prefix (MY_ placeholders), example-name
                   (foo or bar in code); styleGuidance maps each hit rule to its fix
  commands         missingCommands: a script that node, python, bash and similar runners (or ./path) run,
                   found neither beside the file, in its skill folder, in a package folder above it, at its git
                   root, in a workspace the root package.json declares nor in the working directory
                   (missing-file); or an npm, pnpm or yarn script absent from all those package.json files
                   (missing-script). Reads fenced shell blocks and inline code that starts with a runner.
                   <skill-folder> means the nearest folder above the file that holds SKILL.md
--renamed <old>=<new>
                   A renamed file or folder (docs/old.md=docs/new.md), heading (docs/a.md#Old title=New title)
                   or moved section (docs/a.md#Old title=docs/b.md#New title); repeatable. Give headings as text
                   or slug; paths are relative to the working directory. A link that still points at <old> goes
                   to staleInboundAnchors with its replacement and replacementOk (the replacement resolves)
                   instead of brokenLinks or missingAnchors. Scan a folder that holds the inbound links.
--write            Rewrite each stale link whose replacementOk is true and mark it written. Nothing else is
                   ever written.
--root <dir>       Folder that site-absolute links (/docs/a.md) resolve against (default: the file's git root).
--verdicts <list>  Word-list verdicts to report: dont-use,avoid (default), caution, usage, or all.
--senses           Also report terms whose guidance bans one usage only (its first sentence says to refer to,
                   as a verb, for a range, generically, unless and similar); such hits carry scope sense.
--lookup <word>... Print the word-list entries that name each word or phrase; the arguments are words.

Output: JSON on stdout, one finding per line. A check prints ok, checks, counts and the lists of the selected
checks; with --write also written (files and link counts). Every finding has file (relative to the working
directory) and line (1-based). A lookup prints ok and lookup, one {word, entries} row per word, each entry
{term, verdict, scope, guidance}.
Exit: 0 no findings, or a lookup; 1 findings remain; 2 invalid input. Errors: {ok:false,error:{code,message}}.`;

class InputError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

const toPosix = (path) => path.split(sep).join('/');
const escapeRegex = (text) => text.replace(/[\\^$.*+?()[\]{}|/]/g, String.raw`\$&`);
const decode = (text) => {
  try { return decodeURIComponent(text); } catch { return text; }
};

function blankRange(chars, start, end) {
  for (let index = start; index < end; index++) if (chars[index] !== '\n') chars[index] = ' ';
}

export function lineOf(starts, offset) {
  let low = 0;
  let high = starts.length - 1;
  while (low < high) {
    const middle = (low + high + 1) >> 1;
    if (starts[middle] <= offset) low = middle;
    else high = middle - 1;
  }
  return low + 1;
}

// ---------- Markdown model ----------

const INLINE_LINK = /(!?)\[((?:[^[\]\\\n]|\\.|\n(?![ \t]*\n)|\[(?:[^[\]\\\n]|\\.)*\])*)\]\([ \t]*(<[^<>\n]*>|(?:[^\s()\\]|\\.|\((?:[^\s()\\]|\\.)*\))*)(?:\s+(?:"[^"\n]*"|'[^'\n]*'|\([^()\n]*\)))?[ \t]*\)/g;
const DEFINITION = /^ {0,3}\[((?:[^[\]\\\n]|\\.)+)\]:[ \t]*(<[^<>\n]*>|\S+)/gm;
const REFERENCE = /(!?)\[((?:[^[\]\\\n]|\\.)*)\]\[((?:[^[\]\\\n]|\\.)*)\]/g;
const HTML_LINK = /<(?:a|img|source|link|iframe)\b[^>]*?\s(?:href|src)\s*=\s*(["'])([^"'\n]*)\1/gi;
const labelKey = (label) => label.trim().replace(/\s+/g, ' ').toLowerCase();

/** Rendered heading text: code spans, links, emphasis, tags and escapes reduced to what a reader sees. */
export function renderHeading(raw, { code = true } = {}) {
  const spans = [];
  let text = raw.replace(/(`+)([^`]|[^`][\s\S]*?[^`])\1(?!`)/g, (_match, _fence, body) => {
    spans.push(code ? body.replace(/^ ([\s\S]*) $/, '$1') : '');
    return `${MARK}${spans.length - 1}${MARK}`;
  });
  text = text
    .replace(/!\[((?:[^\]\\]|\\.)*)\]\([^)]*\)/g, '$1')
    .replace(/\[((?:[^\]\\]|\\.)*)\]\([^)]*\)/g, '$1')
    .replace(/\[((?:[^\]\\]|\\.)*)\]\[[^\]]*\]/g, '$1')
    .replace(/<\/?[A-Za-z][^<>]*>/g, '');
  for (let pass = 0; pass < 2; pass++) text = text.replace(/(^|[^\p{L}\p{N}_])(_{1,3}|\*{1,3})(?=\S)(.*?\S)\2(?![\p{L}\p{N}_])/gu, '$1$3');
  text = text
    .replace(/~~(.*?)~~/g, '$1')
    .replace(/\\([!-/:-@[-`{-~])/g, '$1')
    .replace(/&(amp|lt|gt|quot|#39|nbsp);/g, (_match, name) => ({ amp: '&', lt: '<', gt: '>', quot: '"', '#39': "'", nbsp: ' ' })[name]);
  return text.replace(new RegExp(`${MARK}(\\d+)${MARK}`, 'g'), (_match, index) => spans[Number(index)]).trim();
}

/** GitHub's heading slug (github-slugger): lowercase, drop punctuation and symbols, spaces become hyphens. */
export function githubSlug(text) {
  return text.toLowerCase().replace(/[^\p{L}\p{M}\p{N}\p{Pc} -]/gu, '').replace(/ /g, '-');
}

/** Text of one Markdown file with code, comments and front matter blanked out, offsets unchanged. */
export function parseMarkdown(raw) {
  const text = raw.replace(/\r/g, ' ');
  const chars = text.split('');
  const starts = [0];
  for (let index = 0; index < text.length; index++) if (text[index] === '\n') starts.push(index + 1);
  const lines = text.split('\n');
  const front = /^---[ \t]*\n(?:[\s\S]*?\n)?(?:---|\.\.\.)[ \t]*(?:\n|$)/.exec(text);
  const frontEnd = front ? front[0].length : 0;
  const hasTitle = Boolean(front && /^title:[ \t]*\S/m.test(front[0]));
  blankRange(chars, 0, frontEnd);
  const blocks = [];
  let open = null;
  for (let index = 0; index < lines.length; index++) {
    const at = starts[index];
    const line = lines[index];
    if (at < frontEnd) continue;
    // A fence indented inside a list item ends with that item: at the first less-indented non-blank line.
    if (open && open.indent > 0 && line.trim() && /^[ \t]*/.exec(line)[0].length < open.indent) open = null;
    if (open) {
      const close = /^[ \t]*(`{3,}|~{3,})[ \t]*$/.exec(line);
      if (close && close[1][0] === open.fence[0] && close[1].length >= open.fence.length) open = null;
      else open.lines.push({ line: index + 1, text: line });
      blankRange(chars, at, at + line.length);
      continue;
    }
    const start = /^[ \t]*(`{3,}|~{3,})(.*)$/.exec(line);
    if (start && !(start[1][0] === '`' && start[2].includes('`'))) {
      open = { fence: start[1], indent: line.length - line.trimStart().length, lang: start[2].trim().split(/\s+/)[0].toLowerCase(), line: index + 1, lines: [] };
      blocks.push(open);
      blankRange(chars, at, at + line.length);
    }
  }
  // Comments and code spans, whichever starts first: a backtick inside a comment opens nothing, and the reverse.
  const spans = [];
  const afterFences = chars.join('');
  for (const match of afterFences.matchAll(/<!--[\s\S]*?(?:-->|$)|(?<![`\\])(`+)(?!`)((?:(?!\n[ \t]*\n)[\s\S])*?[^`])\1(?!`)/g)) {
    if (match[1]) spans.push({ offset: match.index + match[1].length, text: match[2] });
    blankRange(chars, match.index, match.index + match[0].length);
  }
  const prose = chars.join('');
  const doc = { raw, text, starts, lines, prose, blocks, spans, hasTitle };
  doc.headings = headingsOf(doc);
  doc.anchors = anchorsOf(doc);
  doc.links = linksOf(doc);
  doc.words = wordsTextOf(doc);
  return doc;
}

function headingsOf(doc) {
  const proseLines = doc.prose.split('\n');
  const headings = [];
  for (let index = 0; index < proseLines.length; index++) {
    const line = proseLines[index];
    const atx = /^ {0,3}(#{1,6})(?:[ \t]+|$)/.exec(line);
    if (atx) {
      // Measure the marker on the source line: in prose a leading code span is blank and would be swallowed.
      const marker = /^ {0,3}#{1,6}(?:[ \t]+|$)/.exec(doc.lines[index])[0];
      const raw = doc.lines[index].slice(marker.length).replace(/(?:^|[ \t]+)#+[ \t]*$/, '').trim();
      headings.push({ line: index + 1, level: atx[1].length, raw });
      continue;
    }
    const under = proseLines[index + 1];
    const paragraph = line.trim() && !/^ {0,3}(?:[-*+>|#]|\d+[.)])|^ {4}/.test(line) && (index === 0 || !proseLines[index - 1].trim());
    if (paragraph && under !== undefined && /^ {0,3}(?:=+|-+)[ \t]*$/.test(under)) {
      headings.push({ line: index + 1, level: under.trim()[0] === '=' ? 1 : 2, raw: doc.lines[index].trim() });
      index++;
    }
  }
  return headings;
}

function anchorsOf(doc) {
  const anchors = new Set();
  const seen = new Map();
  for (const heading of doc.headings) {
    const base = githubSlug(renderHeading(heading.raw));
    let slug = base;
    while (seen.has(slug)) {
      seen.set(base, seen.get(base) + 1);
      slug = `${base}-${seen.get(base)}`;
    }
    seen.set(slug, 0);
    anchors.add(slug);
    const custom = /\{#([^}\s]+)\}\s*$/.exec(heading.raw);
    if (custom) anchors.add(custom[1]);
  }
  for (const match of doc.prose.matchAll(/<[A-Za-z][^>]*?\s(?:id|name)\s*=\s*["']([^"']+)["']/g)) anchors.add(match[1]);
  return anchors;
}

function linksOf(doc) {
  const { prose } = doc;
  const links = [];
  const definitions = new Set();
  for (const match of prose.matchAll(DEFINITION)) {
    if (match[1].startsWith('^')) continue;
    definitions.add(labelKey(match[1]));
    links.push({ kind: 'definition', offset: match.index, destOffset: match.index + match[0].length - match[2].length, dest: match[2] });
  }
  for (const match of prose.matchAll(INLINE_LINK)) {
    let destOffset = match.index + match[1].length + match[2].length + 3;
    while (prose[destOffset] === ' ' || prose[destOffset] === '\t') destOffset++;
    const textEnd = match.index + match[1].length + match[2].length + 1;
    links.push({ kind: match[1] ? 'image' : 'inline', offset: match.index, textEnd, end: match.index + match[0].length, destOffset, dest: match[3], text: match[1] ? null : match[2] });
  }
  for (const match of prose.matchAll(REFERENCE)) {
    const label = match[3] || match[2];
    if (label.startsWith('^')) continue;
    links.push({ kind: 'reference', offset: match.index, label, defined: definitions.has(labelKey(label)), text: match[1] ? null : match[2] });
  }
  for (const match of prose.matchAll(HTML_LINK)) {
    links.push({ kind: 'html', offset: match.index, destOffset: match.index + match[0].length - 1 - match[2].length, dest: match[2] });
  }
  return links.sort((left, right) => left.offset - right.offset);
}

/** Prose only: link destinations, definitions, tags, URLs and entities blanked; link text stays. */
function wordsTextOf(doc) {
  const chars = doc.prose.split('');
  for (const link of doc.links) if (link.end) blankRange(chars, link.textEnd, link.end);
  const blankAll = (pattern) => {
    for (const match of doc.prose.matchAll(pattern)) blankRange(chars, match.index, match.index + match[0].length);
  };
  blankAll(DEFINITION);
  for (const match of doc.prose.matchAll(REFERENCE)) {
    const at = match.index + match[1].length + match[2].length + 2;
    blankRange(chars, at, match.index + match[0].length);
  }
  blankAll(/<\/?[A-Za-z][^<>]*>/g);
  blankAll(/<[A-Za-z][\w+.-]*:[^<>\s]*>|\b[A-Za-z][\w+.-]*:\/\/[^\s<>)\]]+|\bwww\.[^\s<>)\]]+/g);
  blankAll(/&(?:#\d+|#x[\da-f]+|[a-z]+);/gi);
  return chars.join('');
}

// ---------- Files and paths ----------

function canonical(path) {
  try {
    return realpathSync(path);
  } catch {
    const parent = dirname(path);
    return parent === path ? path : join(canonical(parent), basename(path));
  }
}

function makeContext(cwd, root) {
  const docs = new Map();
  const entries = new Map();
  const gitRoots = new Map();
  const display = (path) => toPosix(relative(cwd, path)) || '.';
  const doc = (path) => {
    const key = canonical(path);
    if (!docs.has(key)) docs.set(key, parseMarkdown(readFileSync(key, 'utf8')));
    return docs.get(key);
  };
  const entriesOf = (dir) => {
    if (!entries.has(dir)) {
      try { entries.set(dir, new Set(readdirSync(dir).map((name) => name.normalize('NFC')))); } catch { entries.set(dir, null); }
    }
    return entries.get(dir);
  };
  // A case-insensitive disk finds README.md as readme.md; hosts like GitHub do not, so compare each segment.
  const pathState = (path) => {
    if (!existsSync(path)) return 'missing';
    for (let current = path; dirname(current) !== current; current = dirname(current)) {
      const names = entriesOf(dirname(current));
      if (names && !names.has(basename(current).normalize('NFC'))) return 'case-mismatch';
    }
    return 'ok';
  };
  const gitRoot = (dir) => {
    if (!gitRoots.has(dir)) {
      const parent = dirname(dir);
      gitRoots.set(dir, existsSync(join(dir, '.git')) ? dir : parent === dir ? null : gitRoot(parent));
    }
    return gitRoots.get(dir);
  };
  const isFile = (path) => {
    try { return statSync(path).isFile(); } catch { return false; }
  };
  const workspaceCache = new Map();
  const workspaces = (dir) => {
    if (!workspaceCache.has(dir)) workspaceCache.set(dir, workspaceDirs(dir));
    return workspaceCache.get(dir);
  };
  return { cwd, root, display, doc, pathState, gitRoot, isFile, workspaces };
}

function collectFiles(paths, cwd) {
  const files = new Map();
  const dirs = new Set();
  const visit = (path, named) => {
    let stats;
    try { stats = statSync(path); } catch {
      if (named) throw new InputError('path-not-found', `No such file or folder: ${path}`);
      return;
    }
    if (stats.isDirectory()) {
      const real = canonical(path);
      if (dirs.has(real)) return;
      dirs.add(real);
      for (const entry of readdirSync(path, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name))) {
        if (entry.name.startsWith('.') || entry.name === 'node_modules') continue;
        visit(join(path, entry.name), false);
      }
      return;
    }
    if (stats.isFile() && (named || MARKDOWN.test(path))) {
      const real = canonical(path);
      if (!files.has(real)) files.set(real, path);
    }
  };
  for (const path of paths) visit(resolve(cwd, path), true);
  return [...files.keys()];
}

// ---------- Renames ----------

export function parseRename(spec, cwd) {
  const equals = spec.indexOf('=');
  if (equals <= 0 || equals === spec.length - 1) throw new InputError('invalid-renamed', `--renamed takes <old>=<new>, got ${JSON.stringify(spec)}.`);
  const before = spec.slice(0, equals);
  const after = spec.slice(equals + 1);
  const oldHash = before.indexOf('#');
  if (oldHash < 0) {
    if (after.includes('#')) throw new InputError('invalid-renamed', `A file rename maps a path to a path; for a heading use <file>#<old heading>=<new heading>: ${spec}`);
    return { spec, kind: 'file', from: canonical(resolve(cwd, before)), to: resolve(cwd, after) };
  }
  const oldPath = before.slice(0, oldHash);
  const oldHeading = before.slice(oldHash + 1);
  const newHash = after.indexOf('#');
  const newPath = newHash < 0 ? oldPath : after.slice(0, newHash) || oldPath;
  const newHeading = newHash < 0 ? after : after.slice(newHash + 1);
  if (!oldPath || !oldHeading.trim() || !newHeading.trim()) throw new InputError('invalid-renamed', `A heading rename names its file and both headings: <file>#<old heading>=<new heading>: ${spec}`);
  return {
    spec,
    kind: 'heading',
    from: canonical(resolve(cwd, oldPath)),
    fromAnchor: githubSlug(renderHeading(oldHeading)),
    to: resolve(cwd, newPath),
    toAnchor: githubSlug(renderHeading(newHeading)),
  };
}

function renameFor(target, anchor, renames) {
  return renames.find((rename) => rename.kind === 'heading'
    ? target === rename.from && anchor === rename.fromAnchor
    : target === rename.from || target.startsWith(`${rename.from}${sep}`));
}

// ---------- Checks ----------

function checkLinks(ctx, file, doc, renames, on, found, stale, counts) {
  const rel = ctx.display(file);
  for (const link of doc.links) {
    const line = lineOf(doc.starts, link.offset);
    if (link.kind === 'reference') {
      if (on.has('links') && !link.defined) found.brokenLinks.push({ file: rel, line, link: `[${link.label}]`, reason: 'undefined-reference' });
      continue;
    }
    const target = (link.dest.startsWith('<') ? link.dest.slice(1, -1) : link.dest).replace(/\\(.)/g, '$1').trim();
    if (!target) {
      if (on.has('links')) found.brokenLinks.push({ file: rel, line, link: link.dest, reason: 'empty-target' });
      continue;
    }
    if (EXTERNAL.test(target)) { counts.externalLinks++; continue; }
    if (TEMPLATED_LINK.test(target)) { counts.skippedLinks++; continue; }
    const hash = target.indexOf('#');
    const written = hash < 0 ? target : target.slice(0, hash);
    const pathPart = decode(written.replace(/\?.*$/, ''));
    const anchor = hash < 0 ? null : decode(target.slice(hash + 1));
    let path = file;
    if (pathPart.startsWith('/')) {
      const base = ctx.root ?? ctx.gitRoot(dirname(file));
      if (!base) { counts.skippedLinks++; continue; }
      path = join(base, pathPart);
    } else if (pathPart) {
      path = resolve(dirname(file), pathPart);
    }
    counts.links++;
    const real = canonical(path);
    const rename = renameFor(real, anchor, renames);
    if (rename) {
      stale.push(staleLink(ctx, { file, rel, line, link, target, written, anchor, real, rename }));
      continue;
    }
    const state = ctx.pathState(path);
    if (state !== 'ok') {
      if (on.has('links')) found.brokenLinks.push({ file: rel, line, link: target, resolved: ctx.display(path), reason: state });
      continue;
    }
    if (!on.has('anchors') || !anchor || /^(?:top|L\d+(?:-L\d+)?)$/.test(anchor) || !MARKDOWN.test(path) || !ctx.isFile(path)) continue;
    const anchors = ctx.doc(path).anchors;
    const wanted = anchor.replace(/^user-content-/, '');
    if (anchors.has(wanted)) continue;
    const guess = [githubSlug(wanted), wanted.toLowerCase()].find((candidate) => anchors.has(candidate));
    found.missingAnchors.push({ file: rel, line, link: target, target: ctx.display(path), anchor, ...(guess ? { suggestion: guess } : {}) });
  }
}

function staleLink(ctx, { file, rel, line, link, target, written, anchor, real, rename }) {
  const moved = rename.kind === 'heading' ? rename.to : `${rename.to}${real.slice(rename.from.length)}`;
  const newAnchor = rename.kind === 'heading' ? rename.toAnchor : anchor;
  const query = /\?[^#]*/.exec(written)?.[0] ?? '';
  let path;
  if (rename.kind === 'heading' && canonical(rename.to) === rename.from) path = written;
  else if (canonical(moved) === file && newAnchor) path = '';
  else if (written.startsWith('/')) path = `/${toPosix(relative(ctx.root ?? ctx.gitRoot(dirname(file)), moved))}${query}`;
  else {
    path = `${toPosix(relative(dirname(file), moved)) || basename(moved)}${query}`;
    if (written.startsWith('./') && !path.startsWith('.')) path = `./${path}`;
  }
  const replacement = `${path}${newAnchor == null ? '' : `#${newAnchor}`}`;
  const resolves = ctx.pathState(moved) === 'ok'
    && (!newAnchor || !MARKDOWN.test(moved) || !ctx.isFile(moved) || ctx.doc(moved).anchors.has(newAnchor));
  const entry = { file: rel, line, link: target, replacement, replacementOk: resolves, renamed: rename.spec };
  return { entry, file, link, replacement };
}

function writeStale(stale) {
  const byFile = new Map();
  for (const item of stale.filter((candidate) => candidate.entry.replacementOk)) {
    if (!byFile.has(item.file)) byFile.set(item.file, []);
    byFile.get(item.file).push(item);
  }
  const written = [];
  for (const [file, items] of byFile) {
    let text = readFileSync(file, 'utf8');
    let count = 0;
    for (const { entry, link, replacement } of items.sort((left, right) => right.link.destOffset - left.link.destOffset)) {
      const { destOffset, dest, kind } = link;
      if (text.slice(destOffset, destOffset + dest.length) !== dest) continue;
      const wrap = kind !== 'html' && (dest.startsWith('<') || /[\s()]/.test(replacement));
      text = `${text.slice(0, destOffset)}${wrap ? `<${replacement}>` : replacement}${text.slice(destOffset + dest.length)}`;
      entry.written = true;
      count++;
    }
    if (count) {
      writeFileSync(file, text);
      written.push({ file: items[0].entry.file, links: count });
    }
  }
  return written;
}

const CALENDAR = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];

/** Words the scanned prose capitalizes mid-sentence, plus capitalized word-list terms: likely proper nouns. */
function properNouns(docs, entries) {
  const names = new Set(CALENDAR);
  for (const { variants, termVariants } of entries) {
    for (const variant of new Set([...variants, ...termVariants])) {
      const words = variant.split(' ');
      words.forEach((word, index) => {
        if ((index > 0 || words.length === 1) && /^\p{Lu}[\p{Ll}\p{M}]+$/u.test(word)) names.add(word);
      });
    }
  }
  for (const doc of docs) {
    const headingLines = new Set(doc.headings.map((heading) => heading.line));
    doc.words.split('\n').forEach((line, index) => {
      if (headingLines.has(index + 1)) return;
      for (const match of line.matchAll(/(?<=[\p{Ll}\p{N}][,;]?[ \t]+)\p{Lu}[\p{Ll}\p{M}]+(?![\p{L}\p{M}\p{N}])/gu)) names.add(match[0]);
    });
  }
  return names;
}

function headingIssuesOf(doc, rel, names) {
  const issues = [];
  const titles = doc.headings.filter((heading) => heading.level === 1);
  if (!titles.length && !doc.hasTitle) issues.push({ file: rel, line: doc.headings[0]?.line ?? 1, rule: 'missing-h1', message: 'The page has no H1 title.' });
  for (const heading of titles.slice(1)) issues.push({ file: rel, line: heading.line, rule: 'multiple-h1', heading: heading.raw, message: `Second H1; the title is on line ${titles[0].line}.` });
  doc.headings.forEach((heading, index) => {
    const previous = doc.headings[index - 1];
    if (previous && heading.level > previous.level + 1) {
      issues.push({ file: rel, line: heading.line, rule: 'skipped-level', heading: heading.raw, message: `H${previous.level} is followed by H${heading.level}.` });
    }
    const shown = renderHeading(heading.raw);
    if (/[.:;,!]$/.test(shown)) issues.push({ file: rel, line: heading.line, rule: 'end-punctuation', heading: heading.raw, message: `Heading ends with "${shown.at(-1)}".` });
    const words = titleCaseWords(renderHeading(heading.raw, { code: false }), names);
    if (words.length) issues.push({ file: rel, line: heading.line, rule: 'sentence-case', heading: heading.raw, words, message: 'Lowercase these words unless they are proper nouns.' });
  });
  return issues;
}

export function titleCaseWords(text, names = new Set()) {
  const words = [];
  let first = true;
  for (const [token] of text.matchAll(/[\p{L}\p{M}\p{N}](?:[\p{L}\p{M}\p{N}'’._/+-]*[\p{L}\p{M}\p{N}])?|[:—–?!.(]|\s-\s/gu)) {
    if (/^(?:[:—–?!.(]|\s-\s)$/u.test(token)) { first = true; continue; }
    if (first) { first = false; continue; }
    if (/^\p{Lu}[\p{Ll}\p{M}]+$/u.test(token) && !names.has(token)) words.push(token);
  }
  return words;
}

// ---------- Word list ----------

export function variantsOf(term) {
  return term
    .replace(/\([^)]*\)/g, ' ')
    .split(',')
    .map((variant) => variant.trim().replace(/^or\s+/, '').replace(/\s*(?:\.\.\.|…)$/, '').replace(/\s+/g, ' ').trim())
    .filter((variant) => /[\p{L}\p{N}&+]/u.test(variant));
}

// A first guidance sentence like these bans one usage of the word, not the word: a reader must judge each hit.
const ONE_SENSE = /\b(?:to refer to|to mean|to describe|to indicate|to address|as an? (?:verb|noun|adjective|synonym|standalone|compound|transitive|intransitive)|as short for|for a range|generically|in isolation|alone|in (?:Android|Compute Engine)|unless|instead of|when (?:describing|expressing))\b|^(?:Use with caution|Use only|OK to use)\b/i;

/** Whether the list bans the word itself ("word") or one usage of it ("sense"). */
export function scopeOf(term, guidance) {
  if (/\((?:verb|noun|adjective)\)/.test(term)) return 'sense';
  return ONE_SENSE.test(guidance.split(/(?<=[.:;])\s/)[0]) ? 'sense' : 'word';
}

const STOP_WORDS = new Set(['a', 'an', 'the', 'on', 'in', 'of', 'to', 'or', 'and', 'for', 'as', 'with', 'its', 'it', 'is', 'be']);
const contentWords = (text) => (text.toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? []).filter((word) => !STOP_WORDS.has(word));
const QUALIFIER = /(?:^|[\s,;]+)(?:if|when|where|whenever|wherever|to|unless|for|in|as|because|instead|which|except|even|particularly|without|alone|that|generically)\b[\s\S]*$/i;

/**
 * Forms the guidance bans instead of the term itself, as in "path: Avoid using filepath, file path, pathname, or
 * path name": the first sentence lists forms, none is a term variant, and each kept form shares a word stem with it.
 */
export function namedForms(variants, guidance) {
  const lead = /^(?:Don't use|Avoid using|Avoid)\s+([\s\S]*)$/i.exec(guidance.split(/(?<=\.)\s/)[0]);
  if (!lead) return null;
  const list = lead[1].replace(/^the following:\s*/i, '').replace(/^the (?:phrase|term|word)s?\s+/i, '').replace(/\([^)]*\)|`/g, ' ')
    .replace(QUALIFIER, '').replace(/[\s,.;:]+$/, '').replace(/\s+/g, ' ').trim();
  const known = variants.map((variant) => variant.toLowerCase());
  if (!list || known.includes(list.toLowerCase())) return null;
  const items = list.split(/\s*,\s*(?:or\s+|and\s+)?|\s+or\s+|\s+and\s+/).map((item) => item.trim()).filter(Boolean);
  if (items.some((item) => known.includes(item.toLowerCase()))) return null;
  const stems = variants.flatMap(contentWords).filter((word) => word.length >= 3);
  const named = items.filter((item) => contentWords(item).some((word) => stems.some((stem) => word.includes(stem) || (word.length >= 3 && stem.includes(word)))));
  return named.length ? named : null;
}

const wordLists = new Map();

export function loadWordList(path = WORD_LIST) {
  if (wordLists.has(path)) return wordLists.get(path);
  const entries = [];
  for (const line of readFileSync(path, 'utf8').split('\n')) {
    if (!line.trim() || line.startsWith('#')) continue;
    const [term, verdict, text = ''] = line.split('\t');
    const guidance = text.trim();
    if (!VERDICTS.includes(verdict)) continue;
    const termVariants = variantsOf(term);
    entries.push({ term, verdict, guidance, scope: scopeOf(term, guidance), termVariants, variants: namedForms(termVariants, guidance) ?? termVariants });
  }
  wordLists.set(path, entries);
  return entries;
}

function variantSource(variant) {
  const body = variant.split(' ').map(escapeRegex).join(String.raw`\s+`);
  if (variant.startsWith('-')) return String.raw`[\p{L}\p{N}]+${body}(?!${JOIN})`;
  const left = /^[\p{L}\p{N}_]/u.test(variant) ? `(?<!${JOIN})` : '';
  const right = /[\p{L}\p{N}_]$/u.test(variant) ? `(?!${JOIN})` : '';
  return `${left}${body}${right}`;
}

/** One alternation per letter-case mode; the longest variant wins at a position. */
function wordMatcher(entries, verdicts, senses) {
  const groups = [];
  for (const sensitive of [true, false]) {
    const byVariant = new Map();
    for (const entry of entries.filter((candidate) => verdicts.includes(candidate.verdict) && (senses || candidate.scope === 'word'))) {
      for (const variant of entry.variants) {
        if (/\p{Lu}/u.test(variant) !== sensitive) continue;
        const key = sensitive ? variant : variant.toLowerCase();
        if (!byVariant.has(key)) byVariant.set(key, { variant, entries: [] });
        byVariant.get(key).entries.push(entry);
      }
    }
    const list = [...byVariant.values()].sort((left, right) => right.variant.length - left.variant.length);
    if (list.length) groups.push({ list, pattern: new RegExp(list.map((item) => `(${variantSource(item.variant)})`).join('|'), sensitive ? 'gu' : 'giu') });
  }
  return (text) => {
    const hits = [];
    const normalized = text.replace(/[‘’]/g, "'");
    for (const { list, pattern } of groups) {
      for (const match of normalized.matchAll(pattern)) {
        const index = match.findIndex((group, position) => position > 0 && group !== undefined);
        for (const entry of list[index - 1].entries) hits.push({ offset: match.index, word: text.slice(match.index, match.index + match[0].length).replace(/\s+/g, ' '), entry });
      }
    }
    return hits.sort((left, right) => left.offset - right.offset);
  };
}

export function lookupWords(words, path = WORD_LIST) {
  const entries = loadWordList(path);
  // Whole words: a phrase matches inside another only at space boundaries.
  const padded = (text) => ` ${text.trim().toLowerCase().split(/\s+/).join(' ')} `;
  return {
    ok: true,
    lookup: words.map((word) => {
      const wanted = padded(word);
      const matches = entries.filter(({ variants, termVariants }) => [...variants, ...termVariants].some((variant) => {
        const known = padded(variant);
        return known.includes(wanted) || wanted.includes(known);
      }));
      return { word, entries: matches.map(({ term, verdict, scope, guidance }) => ({ term, verdict, scope, guidance })) };
    }),
  };
}

// ---------- Style rules ----------

const WORD_EDGE_LEFT = String.raw`(?<![\p{L}\p{N}_-])`;
const WORD_EDGE_RIGHT = String.raw`(?![\p{L}\p{N}_-])`;
// The style guide's fixed replacements; the word list's guidance covers everything else.
const WORD_CHOICES = [
  ['allows? you to', 'lets you'], ['utiliz(?:e|es|ed|ing|ation)', 'use'], ['leverag(?:e|es|ed|ing)', 'use'],
  ['click\\s+on', 'click'], ['abort(?:s|ed|ing)?', 'stop'], ['kill(?:s|ed|ing)?', 'stop'],
  ['white-?list(?:s|ed|ing)?', 'allowlist'], ['black-?list(?:s|ed|ing)?', 'denylist'], ['masters?', 'primary'],
  ['slaves?', 'replica'], ['sanity\\s+checks?', 'quick check'], ['(?:you\\s+)?guys', 'everyone'], ['and/or', 'A or B, or both'],
].map(([pattern, use]) => ({ pattern: new RegExp(`(?<!${JOIN})${pattern}(?!${JOIN})`, 'giu'), use }));
const PROSE_RULES = [
  { rule: 'filler', pattern: new RegExp(`${WORD_EDGE_LEFT}(?:simply|easy|easily|of\\s+course)${WORD_EDGE_RIGHT}`, 'giu'), fix: 'Delete the filler word or state the fact it softens.' },
  { rule: 'filler', pattern: /(?<=[\p{L}\p{N})"'])!(?=\s|$)/gu, fix: 'End the sentence with a period, not an exclamation mark.' },
  { rule: 'should-may', pattern: new RegExp(`${WORD_EDGE_LEFT}(?:should|may)${WORD_EDGE_RIGHT}(?!\\s+\\d)`, 'giu'), fix: 'Required: must or the imperative; recommended: we recommend; optional: can.' },
  { rule: 'curly-quote', pattern: /[“”‘’]/gu, fix: 'Use straight quotes and apostrophes.' },
  { rule: 'date-format', pattern: /(?<![\p{N}/.])\d{1,2}\/\d{1,2}\/\d{2}(?:\d{2})?(?![\p{N}/])/gu, fix: 'Write January 19, 2017, or ISO 2017-01-19.' },
  { rule: 'time-format', pattern: /(?<![\p{N}:.])\d{1,2}(?::\d{2})?(?:\s?(?:am|pm|a\.m\.|p\.m\.)|AM|PM)(?![\p{L}])/gu, fix: 'Write times as 3 PM or 3:30 PM.' },
  { rule: 'placeholder-prefix', pattern: /(?<![\p{L}\p{N}_])MY_[A-Z\d_]+/gu, fix: 'Name a placeholder for what it holds (PROJECT_ID), not MY_ID.' },
];
const CODE_RULES = [
  PROSE_RULES.find((rule) => rule.rule === 'placeholder-prefix'),
  // bar alone is often real (a bar chart); it is a placeholder name beside foo, baz or qux.
  { rule: 'example-name', when: new RegExp(`${WORD_EDGE_LEFT}(?:foo|baz|qux)${WORD_EDGE_RIGHT}`, 'u'), pattern: new RegExp(`${WORD_EDGE_LEFT}(?:foo|bar|baz|qux)${WORD_EDGE_RIGHT}`, 'gu'), fix: 'Use a descriptive example value, not foo or bar.' },
];
const VAGUE_LINK_TEXT = new Set(['here', 'click here', 'this', 'this link', 'link', 'this page', 'page', 'read more', 'more', 'go here']);
const VAGUE_FIX = 'Make the link text name the destination.';

function styleIssuesOf(doc, rel, guidance, taken) {
  const issues = [];
  const add = (offset, rule, text, fix, extra = {}) => {
    issues.push({ file: rel, line: lineOf(doc.starts, offset), rule, text, ...extra });
    guidance[rule] = fix;
    taken.add(offset);
  };
  for (const { pattern, use } of WORD_CHOICES) {
    for (const match of doc.words.matchAll(pattern)) add(match.index, 'word-choice', match[0], 'Write the replacement the style guide names.', { replacement: use });
  }
  for (const { rule, pattern, fix } of PROSE_RULES) for (const match of doc.words.matchAll(pattern)) add(match.index, rule, match[0], fix);
  for (const link of doc.links) {
    if (link.text == null) continue;
    const shown = renderHeading(link.text).toLowerCase().replace(/\s+/g, ' ').replace(/[.:!]+$/, '');
    if (VAGUE_LINK_TEXT.has(shown)) add(link.offset, 'vague-link-text', link.text, VAGUE_FIX);
  }
  const code = [
    ...doc.spans.map((span) => ({ offset: span.offset, text: span.text })),
    ...doc.blocks.flatMap((block) => block.lines.map(({ line, text }) => ({ offset: doc.starts[line - 1], text }))),
  ];
  for (const { offset, text } of code) {
    for (const { rule, when, pattern, fix } of CODE_RULES) {
      if (when && !when.test(text)) continue;
      for (const match of text.matchAll(pattern)) add(offset + match.index, rule, match[0], fix);
    }
  }
  return issues.sort((left, right) => left.line - right.line);
}

// ---------- Commands ----------

const SHELL_LANGS = new Set(['', 'bash', 'sh', 'shell', 'zsh', 'console', 'shell-session', 'shellsession', 'terminal']);
const RUNNERS = new Set(['node', 'python', 'python3', 'bash', 'sh', 'zsh', 'deno', 'bun', 'tsx', 'ts-node']);
const PACKAGE_MANAGERS = new Set(['npm', 'pnpm', 'yarn']);
const WRAPPERS = new Set(['sudo', 'env', 'time', 'exec', 'command', 'nohup', 'gtimeout', 'timeout']);
// Flags after which the runner reads code or a module name, not a script path.
const CODE_FLAGS = new Set(['--eval', '--print', '--test', '-']);
const CODE_SHORT_FLAGS = { node: 'ep', bash: 'c', sh: 'c', zsh: 'c', python: 'cm', python3: 'cm', bun: 'e', deno: 'e' };
const VALUE_FLAGS = new Set(['-r', '--require', '--import', '--loader', '--experimental-loader', '--env-file', '--input-type', '-C', '--conditions', '--title']);
const YARN_COMMANDS = new Set(['add', 'audit', 'autoclean', 'bin', 'cache', 'check', 'config', 'constraints', 'create', 'dedupe', 'dlx', 'exec', 'explain', 'generate-lock-entry', 'global', 'help', 'import', 'info', 'init', 'install', 'licenses', 'link', 'list', 'login', 'logout', 'node', 'npm', 'outdated', 'owner', 'pack', 'patch', 'patch-commit', 'plugin', 'policy', 'publish', 'rebuild', 'remove', 'run', 'search', 'set', 'stage', 'tag', 'team', 'unlink', 'unplug', 'up', 'upgrade', 'upgrade-interactive', 'version', 'why', 'workspace', 'workspaces']);
// Inline code is a command only when it starts with a runner; a bare ./path there is usually a path.
const COMMAND_START = /^\s*(?:[A-Za-z_]\w*=\S*\s+)*(?:node|python3?|bash|sh|zsh|deno|bun|tsx|ts-node|npm|pnpm|yarn)\b/;

function commandLines(doc) {
  const commands = [];
  for (const [group, block] of doc.blocks.entries()) {
    if (!SHELL_LANGS.has(block.lang)) continue;
    let pending = null;
    for (const { line, text } of block.lines) {
      const joined = pending ? `${pending.text} ${text.trim()}` : text;
      const first = pending ? pending.line : line;
      if (/\\\s*$/.test(joined)) {
        pending = { line: first, text: joined.replace(/\\\s*$/, '') };
        continue;
      }
      pending = null;
      commands.push({ group, line: first, text: joined.replace(/^\s*[$>]\s+/, '') });
    }
  }
  doc.spans.forEach((span, index) => {
    if (COMMAND_START.test(span.text)) commands.push({ group: `span-${index}`, line: lineOf(doc.starts, span.offset), text: span.text });
  });
  return commands;
}

function segmentsOf(text) {
  const segments = [[]];
  for (const match of text.matchAll(/"((?:[^"\\]|\\.)*)"|'([^']*)'|(\S+)/g)) {
    if (match[3] === undefined) {
      segments.at(-1).push(match[1] ?? match[2]);
      continue;
    }
    if (match[3].startsWith('#')) break;
    for (const piece of match[3].split(/(&&|\|\||[;|&(){}])/)) {
      if (!piece) continue;
      if (/^(?:&&|\|\||[;|&(){}])$/.test(piece)) segments.push([]);
      else segments.at(-1).push(piece);
    }
  }
  return segments.filter((segment) => segment.length);
}

function invocationOf(tokens) {
  for (let index = 0; index < tokens.length; index++) {
    const token = tokens[index];
    const name = basename(token);
    if (RUNNERS.has(name)) return scriptOf(tokens, index + 1, name);
    if (PACKAGE_MANAGERS.has(name)) return packageScriptOf(tokens, index + 1, name);
    if (/^\.\.?\//.test(token)) return { kind: 'file', path: token };
    if (/^[A-Za-z_]\w*=/.test(token) || WRAPPERS.has(name) || /^\d+[smh]?$/.test(token) || token.startsWith('-')) continue;
    return null;
  }
  return null;
}

function scriptOf(tokens, start, runner) {
  let index = start;
  if ((runner === 'deno' && ['run', 'test'].includes(tokens[index])) || (runner === 'bun' && tokens[index] === 'run')) index++;
  for (; index < tokens.length; index++) {
    const token = tokens[index];
    if (CODE_FLAGS.has(token)) return null;
    if (/^-[A-Za-z]+$/.test(token) && [...(CODE_SHORT_FLAGS[runner] ?? '')].some((flag) => token.includes(flag))) return null;
    if (VALUE_FLAGS.has(token)) { index++; continue; }
    if (token.startsWith('-')) continue;
    return /[/\\]/.test(token) || /\.(?:[mc]?[jt]s|py|sh|bash|zsh)$/.test(token) ? { kind: 'file', path: token } : null;
  }
  return null;
}

function packageScriptOf(tokens, start, manager) {
  const command = tokens[start];
  if (!command || command.startsWith('-') || TEMPLATED_COMMAND.test(command)) return null;
  if (command === 'run' || command === 'run-script') {
    const name = tokens[start + 1];
    return name && !name.startsWith('-') && !TEMPLATED_COMMAND.test(name) ? { kind: 'script', name } : null;
  }
  if (manager === 'npm' && command === 'test') return { kind: 'script', name: 'test' };
  if (manager === 'yarn' && !YARN_COMMANDS.has(command)) return { kind: 'script', name: command, binary: true };
  return null;
}

function skillFolder(file) {
  for (let dir = dirname(file); ; dir = dirname(dir)) {
    if (existsSync(join(dir, 'SKILL.md'))) return dir;
    if (dirname(dir) === dir) return null;
  }
}

const readJson = (path) => {
  try { return JSON.parse(readFileSync(path, 'utf8')); } catch { return null; }
};

/** Package folders the root package.json declares as workspaces (exact folders and one-level dir/* globs). */
function workspaceDirs(root) {
  if (!root) return [];
  const declared = readJson(join(root, 'package.json'))?.workspaces;
  const patterns = Array.isArray(declared) ? declared : Array.isArray(declared?.packages) ? declared.packages : [];
  const dirs = [];
  for (const pattern of patterns.filter((item) => typeof item === 'string' && !item.startsWith('!'))) {
    const glob = /^(.*)\/\*{1,2}$/.exec(pattern);
    if (!glob) { dirs.push(resolve(root, pattern)); continue; }
    const parent = resolve(root, glob[1]);
    try {
      for (const entry of readdirSync(parent, { withFileTypes: true })) if (entry.isDirectory()) dirs.push(join(parent, entry.name));
    } catch { /* a pattern that matches nothing */ }
  }
  return dirs.filter((dir) => existsSync(join(dir, 'package.json')));
}

function missingCommandsOf(ctx, file, doc, rel) {
  const missing = [];
  const fileDir = dirname(file);
  const git = ctx.gitRoot(fileDir);
  // Package folders from the file up to its git root, then the working directory and the root's workspaces.
  const packageDirs = [];
  for (let dir = fileDir; ; dir = dirname(dir)) {
    if (existsSync(join(dir, 'package.json'))) packageDirs.push(dir);
    if (dir === git || dirname(dir) === dir) break;
  }
  if (existsSync(join(ctx.cwd, 'package.json'))) packageDirs.push(ctx.cwd);
  packageDirs.push(...ctx.workspaces(git));
  const packages = [...new Set(packageDirs)];
  // Skill docs name scripts relative to the skill folder (the one holding SKILL.md).
  const bases = [...new Set([fileDir, skillFolder(file), ...packages, git, ctx.cwd, ctx.root].filter(Boolean))];
  const scriptsOf = (dir) => {
    return readJson(join(dir, 'package.json'))?.scripts ?? {};
  };
  // A `cd <dir>` moves later commands of the same block; they resolve from there as well.
  const moved = new Map();
  for (const { group, line, text } of commandLines(doc)) {
    for (const segment of segmentsOf(text)) {
      const from = moved.get(group) ?? bases;
      if (segment[0] === 'cd' && segment[1] && !TEMPLATED_COMMAND.test(segment[1])) {
        moved.set(group, [...new Set([...from, ...from.map((base) => resolve(base, segment[1]))])]);
        continue;
      }
      const call = invocationOf(segment);
      if (!call) continue;
      const command = segment.join(' ');
      if (call.kind === 'file') {
        let path = call.path;
        if (path.includes('<skill-folder>')) {
          const folder = skillFolder(file);
          if (!folder) continue;
          path = path.replace('<skill-folder>', folder);
        }
        if (TEMPLATED_COMMAND.test(path)) continue;
        const found = isAbsolute(path) ? existsSync(path) : from.some((base) => existsSync(resolve(base, path)));
        if (!found) missing.push({ file: rel, line, command, missing: call.path, reason: 'missing-file' });
        continue;
      }
      if (!packages.length) continue;
      const present = packages.some((dir) => Object.hasOwn(scriptsOf(dir), call.name)
        || (call.binary && existsSync(join(dir, 'node_modules', '.bin', call.name))));
      if (!present) missing.push({ file: rel, line, command, missing: call.name, reason: 'missing-script', packageJson: packages.map((dir) => ctx.display(join(dir, 'package.json'))) });
    }
  }
  return missing;
}

// ---------- Entry ----------

function listOf(value, allowed, name) {
  const items = value.split(',').map((item) => item.trim()).filter(Boolean);
  if (items.length === 1 && items[0] === 'all') return [...allowed];
  const unknown = items.filter((item) => !allowed.includes(item));
  if (!items.length || unknown.length) throw new InputError(`invalid-${name}`, `--${name} takes a comma-separated subset of ${allowed.join(',')} or all; got ${value}.`);
  return allowed.filter((item) => items.includes(item));
}

export function parseArgs(argv) {
  const options = { paths: [], renamed: [], write: false, root: null, checks: [...CHECKS], verdicts: [...DEFAULT_VERDICTS], senses: false, lookup: false, help: false };
  const args = argv.flatMap((arg) => /^--[a-z]+=/.test(arg) ? [arg.slice(0, arg.indexOf('=')), arg.slice(arg.indexOf('=') + 1)] : [arg]);
  const value = (index, flag) => {
    const next = args[index + 1];
    if (next === undefined || next.startsWith('--')) throw new InputError('missing-value', `${flag} needs a value; see --help.`);
    return next;
  };
  for (let index = 0; index < args.length; index++) {
    const arg = args[index];
    if (arg === '--help' || arg === '-h') options.help = true;
    else if (arg === '--write') options.write = true;
    else if (arg === '--lookup') options.lookup = true;
    else if (arg === '--senses') options.senses = true;
    else if (arg === '--renamed') options.renamed.push(value(index++, arg));
    else if (arg === '--root') options.root = value(index++, arg);
    else if (arg === '--checks') options.checks = listOf(value(index++, arg), CHECKS, 'checks');
    else if (arg === '--verdicts') options.verdicts = listOf(value(index++, arg), VERDICTS, 'verdicts');
    else if (arg.startsWith('-') && arg !== '-') throw new InputError('unknown-option', `Unknown option ${arg}; see --help.`);
    else options.paths.push(arg);
  }
  return options;
}

export function checkDocs({ paths, renamed = [], write = false, root = null, checks = CHECKS, verdicts = DEFAULT_VERDICTS, senses = false, cwd = process.cwd(), wordList = WORD_LIST }) {
  if (!paths.length) throw new InputError('missing-input', 'Pass Markdown files or folders; see --help.');
  let rootDir = null;
  if (root) {
    rootDir = resolve(cwd, root);
    if (!existsSync(rootDir) || !statSync(rootDir).isDirectory()) throw new InputError('invalid-root', `--root is not a folder: ${String(root)}`);
  }
  const ctx = makeContext(cwd, rootDir);
  const renames = renamed.map((spec) => parseRename(spec, cwd));
  const files = collectFiles(paths, cwd);
  const on = new Set(checks);
  const entries = on.has('words') || on.has('headings') ? loadWordList(wordList) : [];
  const docs = files.map((file) => ({ file, rel: ctx.display(file), doc: ctx.doc(file) }));
  const names = on.has('headings') ? properNouns(docs.map(({ doc }) => doc), entries) : null;
  const scanWords = on.has('words') ? wordMatcher(entries, verdicts, senses) : null;
  const found = { brokenLinks: [], missingAnchors: [], headingIssues: [], wordListHits: [], styleIssues: [], missingCommands: [] };
  const stale = [];
  const wordGuidance = {};
  const styleGuidance = {};
  const counts = { files: files.length, links: 0, externalLinks: 0, skippedLinks: 0 };
  for (const { file, rel, doc } of docs) {
    if (on.has('links') || on.has('anchors') || renames.length) checkLinks(ctx, file, doc, renames, on, found, stale, counts);
    if (names) found.headingIssues.push(...headingIssuesOf(doc, rel, names));
    // A word a style rule reports (with its replacement) is not listed twice.
    const taken = new Set();
    if (on.has('style')) found.styleIssues.push(...styleIssuesOf(doc, rel, styleGuidance, taken));
    if (scanWords) {
      for (const { offset, word, entry } of scanWords(doc.words)) {
        if (taken.has(offset)) continue;
        found.wordListHits.push({ file: rel, line: lineOf(doc.starts, offset), word, term: entry.term, rule: entry.verdict, ...(entry.scope === 'sense' ? { scope: 'sense' } : {}) });
        wordGuidance[entry.term] = entry.guidance;
      }
    }
    if (on.has('commands')) found.missingCommands.push(...missingCommandsOf(ctx, file, doc, rel));
  }
  const written = write ? writeStale(stale) : null;
  const lists = {
    brokenLinks: on.has('links') ? found.brokenLinks : null,
    missingAnchors: on.has('anchors') ? found.missingAnchors : null,
    staleInboundAnchors: on.has('links') || on.has('anchors') || renames.length ? stale.map((item) => item.entry) : null,
    headingIssues: on.has('headings') ? found.headingIssues : null,
    wordListHits: on.has('words') ? found.wordListHits : null,
    wordGuidance: on.has('words') ? wordGuidance : null,
    styleIssues: on.has('style') ? found.styleIssues : null,
    styleGuidance: on.has('style') ? styleGuidance : null,
    missingCommands: on.has('commands') ? found.missingCommands : null,
  };
  for (const [key, list] of Object.entries(lists)) if (Array.isArray(list)) counts[key] = list.length;
  const open = Object.values(lists).filter(Array.isArray).flat().filter((item) => !item.written).length;
  const result = { ok: open === 0, checks: CHECKS.filter((check) => on.has(check)), counts };
  for (const [key, list] of Object.entries(lists)) if (list) result[key] = list;
  if (written) result.written = written;
  return result;
}

/** JSON with one array item or object member per line, so long finding lists stay scannable. */
export function formatResult(result) {
  const members = Object.entries(result).map(([key, value]) => {
    if (Array.isArray(value) && value.length) return `  ${JSON.stringify(key)}: [\n${value.map((item) => `    ${JSON.stringify(item)}`).join(',\n')}\n  ]`;
    if (value && typeof value === 'object' && !Array.isArray(value) && Object.keys(value).length) {
      return `  ${JSON.stringify(key)}: {\n${Object.entries(value).map(([name, item]) => `    ${JSON.stringify(name)}: ${JSON.stringify(item)}`).join(',\n')}\n  }`;
    }
    return `  ${JSON.stringify(key)}: ${JSON.stringify(value)}`;
  });
  return `{\n${members.join(',\n')}\n}`;
}

if (isMainModule(import.meta.url)) {
  let options;
  try {
    options = parseArgs(process.argv.slice(2));
  } catch (error) {
    fail(error.code ?? 'invalid-input', errorMessage(error), 2);
  }
  if (options.help) {
    console.log(USAGE);
    process.exit(0);
  }
  let result;
  try {
    if (options.lookup && !options.paths.length) throw new InputError('missing-input', '--lookup needs at least one word.');
    result = options.lookup ? lookupWords(options.paths) : checkDocs(options);
  } catch (error) {
    if (error instanceof InputError) fail(error.code, error.message, 2);
    fail('check-failed', errorMessage(error), 1);
  }
  console.log(formatResult(result));
  process.exitCode = result.ok ? 0 : 1;
}
