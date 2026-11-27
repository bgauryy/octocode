#!/usr/bin/env node
// Trace an RFC saved in the format of ../output.md: IDs, feature/task/check coverage, task order,
// the critical path, citations against Sources, local links, and the task graph as Mermaid.
import { existsSync, readFileSync, statSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { isMainModule } from './lib/entrypoint.mjs';
import { errorMessage, fail } from './lib/fail.mjs';
import { mdKit } from './lib/report/markdown.mjs';
import { collect, resolveRefs } from './lib/report/workspace.mjs';

const HELP = `Trace an RFC written in the octocode-rfc-generator format (output.md). Reads only; writes nothing.

Usage:
  node scripts/rfc-trace.mjs <RFC.md> [--workspace <dir>] [--not-id <ID>[,<ID>...]]...
  node scripts/rfc-trace.mjs <RFC.md> --mermaid [--workspace <dir>]
  node scripts/rfc-trace.mjs --help

Input: one Markdown RFC with the H2 sections Features, Tasks and rollout, Validation and success and
Sources (Prerequisites, Decisions and reasoning and Open questions are read when present).
  P, F, T, V IDs are defined by the first cell of a table row in Prerequisites, Features, Tasks and
  rollout, Validation and success. V IDs are also defined in Validation and success, and D and Q IDs
  anywhere, by a line, list item or heading labelled with the ID ("**V1 (F1):** given ...",
  "**D1: choice.**", "- Q1 Blocking: ..."), and Q IDs by a "Blocking (Q1):" label. A V names the
  F IDs in its row or labelled item.
  Every other P/F/D/T/V/Q token is a reference; "F1–F3", "F1-F3" and "F1 to F3" name each ID between.
  Fenced code is skipped except mermaid blocks; link targets and URLs are not scanned for IDs.
  Tasks tables need a "Serves" and a "Depends on" column; a "State" column labels graph nodes and a
  Features "Priority" of Won't takes the feature out of the task and check trace.
  --workspace <dir>  Second base for local links (default: the folder holding .octocode/, else the git
                     root, else the current directory); links resolve from the RFC's folder first.
  --not-id <ID>      A token that matches the ID shape but is no RFC ID (P99 latency, Q3 of a year).
  --mermaid          Print only the task graph as a fenced mermaid block (text, not JSON).

Output (JSON on stdout):
  ok                     true when every finding list below is empty
  file, workspace        absolute paths used
  undefinedIds           [{id, lines}] referenced but never defined
  duplicateIds           [{id, lines}] defined more than once
  featuresWithoutTask    [F] no task's Serves names it
  featuresWithoutCheck   [F] no V row names it
  tasksWithoutServes     [T] Serves names no defined F or P
  forwardDeps            [{task, dependsOn, line}] a task depends on one listed after it
  cycles                 [[T...]] tasks that depend on each other (table order)
  criticalPath           [T...] the longest Depends on chain, first to last; [] when a cycle exists
  taskGraphBranches      false when criticalPath holds every task (a numbered list replaces the graph)
  citationsNotInSources  [{target, lines}] links outside Sources whose source Sources does not link
  brokenLocalLinks       [{target, line, reason}] reason: notFound, anchorNotFound, lineBeyondEnd,
                         referenceUndefined, footnoteUndefined. Remote URLs are not fetched.
  taskGraphMermaid       flowchart of T IDs, Depends on edges, critical path edges labelled critical

Exit codes: 0 no findings (and always for --mermaid), 1 findings, 2 invalid input: unreadable file,
missing section, a Tasks table without Serves or Depends on, no F or T rows, or a bad argument
({ok:false, error:{code, message}}).
`;

const SECTIONS = {
  prerequisites: 'Prerequisites',
  features: 'Features',
  tasks: 'Tasks and rollout',
  validation: 'Validation and success',
  sources: 'Sources',
};
const REQUIRED = [SECTIONS.features, SECTIONS.tasks, SECTIONS.validation, SECTIONS.sources];
const TABLE_SECTION = { P: SECTIONS.prerequisites, F: SECTIONS.features, T: SECTIONS.tasks, V: SECTIONS.validation };
const ID_SHAPE = /^[PFDTVQ]\d+$/;
const ID_TOKEN = /\b([PFDTVQ])(\d+)(?:(?:\s*[–—-]\s*|\s+(?:to|through)\s+)\1(\d+))?\b/g;
// Emphasis and code marks around a label are one character class: each run of them matches one way only.
const LABEL_START = /^ {0,3}(?:#{1,6}\s+|[-*+]\s+(?:\[[ xX]\]\s+)?|\d+[.)]\s+)?[*_`]*([DQV]\d+)(?:\s*\([^)]*\))?[*_`]*\s*(?:[:.—–]|-\s|\s+(?:Blocking|Non-blocking)\b)/;
const LIST_ITEM = /^(\s*)(?:[-*+]|\d+[.)])\s/;
const BLOCKING_LABEL = /^ {0,3}(?:[-*+]\s+|\d+[.)]\s+)?(?:\*\*|__)?(?:Blocking|Non-blocking)\b([^:]*):/;
const DELIMITER_ROW = /^ {0,3}\|?\s*:?-+:?\s*(?:\|\s*:?-+:?\s*)*\|?\s*$/;
const INLINE_LINK = /(!?)\[((?:\\.|[^\\[\]]|\[(?:\\.|[^\\[\]])*\])*)\]\(\s*(<[^<>\n]*>|(?:\\.|[^\s()\\]|\((?:\\.|[^\s()\\])*\))*)(?:\s+(?:"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|\((?:\\.|[^()\\])*\)))?\s*\)/g;
const REFERENCE_LINK = /(!?)\[((?:\\.|[^\\[\]])+)\]\[((?:\\.|[^\\[\]])*)\]/g;
const SHORTCUT_LINK = /(!?)\[((?:\\.|[^\\[\]^])(?:\\.|[^\\[\]])*)\](?![([:])/g;
const FOOTNOTE_REF = /\[\^([^\]\s]+)\]/g;
const AUTOLINK = /<([a-z][\w+.-]{1,31}:[^\s<>]*)>/gi;
const BARE_URL = /\bhttps?:\/\/[^\s<>]+/g;
const REFERENCE_DEF = /^ {0,3}\[((?:\\.|[^\\\]^])(?:\\.|[^\\\]])*)\]:\s*(<[^>\n]*>|\S+)/;
const FOOTNOTE_DEF = /^ {0,3}\[\^([^\]]+)\]:/;
const STATES = new Set(['todo', 'doing', 'blocked', 'done']);

class InputError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

const kit = mdKit();
const blank = (text) => ' '.repeat(text.length);
const labelKey = (label) => label.trim().replace(/\s+/g, ' ').toLowerCase();
const decode = (value) => {
  try { return decodeURIComponent(value); } catch { return value; }
};
/** Cell or heading text without inline Markdown: link text kept, emphasis and code marks dropped. */
const plain = (text) => text
  .replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
  .replace(/(\*\*|__|`)/g, '')
  .replace(/^[*_]+|[*_]+$/g, '')
  .trim();

/** IDs in text, in order of first appearance; ranges ("F1–F3") name every ID between. */
export function idsIn(text, ignore = new Set()) {
  const out = [];
  const add = (id) => { if (!ignore.has(id) && !out.includes(id)) out.push(id); };
  for (const [, letter, start, end] of text.matchAll(ID_TOKEN)) {
    const from = Number(start);
    const to = end === undefined ? from : Number(end);
    if (to <= from) {
      add(`${letter}${start}`);
      if (end !== undefined) add(`${letter}${end}`);
      continue;
    }
    for (let n = from; n <= to; n++) add(`${letter}${String(n).padStart(start.length, '0')}`);
  }
  return out;
}

function splitCells(line) {
  let body = line.trim();
  if (body.startsWith('|')) body = body.slice(1);
  if (body.endsWith('|') && !body.endsWith('\\|')) body = body.slice(0, -1);
  const cells = [];
  let cell = '';
  for (let i = 0; i < body.length; i++) {
    if (body[i] === '\\' && body[i + 1] === '|') { cell += '|'; i++; continue; }
    if (body[i] === '|') { cells.push(cell.trim()); cell = ''; continue; }
    cell += body[i];
  }
  cells.push(cell.trim());
  return cells;
}

/** Line records: kind (text, front, fence, mermaid), H2 section, and the text with comments removed. */
function scanLines(text) {
  const raw = text.split(/\r?\n/);
  const lines = [];
  let fence = null;
  let lang = '';
  let comment = false;
  let section = null;
  let front = raw[0]?.trim() === '---';
  for (let i = 0; i < raw.length; i++) {
    const line = raw[i];
    if (front) {
      lines.push({ n: i + 1, kind: 'front', text: line, section });
      if (i > 0 && /^(---|\.\.\.)\s*$/.test(line.trim())) front = false;
      continue;
    }
    const marker = line.match(/^ {0,3}(`{3,}|~{3,})\s*([^\s`]*)/);
    if (fence) {
      if (marker && marker[1][0] === fence[0] && marker[1].length >= fence.length && !line.trim().slice(marker[1].length).trim()) {
        fence = null;
        lines.push({ n: i + 1, kind: 'fence', text: '', section });
      } else lines.push({ n: i + 1, kind: lang === 'mermaid' ? 'mermaid' : 'fence', text: line, section });
      continue;
    }
    if (marker) {
      fence = marker[1];
      lang = marker[2].toLowerCase();
      lines.push({ n: i + 1, kind: 'fence', text: '', section });
      continue;
    }
    let visible = '';
    let rest = line;
    while (rest) {
      if (comment) {
        const close = rest.indexOf('-->');
        if (close < 0) { rest = ''; break; }
        comment = false;
        rest = rest.slice(close + 3);
      } else {
        const open = rest.indexOf('<!--');
        if (open < 0) { visible += rest; break; }
        visible += rest.slice(0, open);
        comment = true;
        rest = rest.slice(open + 4);
      }
    }
    const heading = visible.match(/^ {0,3}(#{1,6})\s+(.+?)\s*#*\s*$/);
    if (heading && heading[1].length <= 2) section = heading[1].length === 2 ? plain(heading[2]) : null;
    lines.push({ n: i + 1, kind: 'text', text: visible, section });
  }
  return lines;
}

function findTables(lines) {
  const tables = [];
  for (let i = 0; i + 1 < lines.length; i++) {
    const head = lines[i];
    const delimiter = lines[i + 1];
    if (head.kind !== 'text' || delimiter.kind !== 'text' || !head.text.includes('|') || !DELIMITER_ROW.test(delimiter.text) || !delimiter.text.includes('-')) continue;
    const headers = splitCells(head.text).map((cell) => plain(cell).toLowerCase());
    const rows = [];
    let j = i + 2;
    for (; j < lines.length && lines[j].kind === 'text' && lines[j].text.trim() && lines[j].text.includes('|'); j++) {
      rows.push({ n: lines[j].n, cells: splitCells(lines[j].text), line: lines[j] });
      lines[j].table = true;
    }
    head.table = true;
    delimiter.table = true;
    tables.push({ section: head.section, n: head.n, headers, rows });
    i = j - 1;
  }
  return tables;
}

const exactId = (cell) => {
  const id = plain(cell ?? '');
  return ID_SHAPE.test(id) ? id : null;
};
const column = (headers, pattern) => headers.findIndex((header) => pattern.test(header));

/** Text scanned for ID references: link text kept, link targets and URLs dropped. */
const idText = (text) => text.replace(INLINE_LINK, (m, bang, label) => ` ${label} `).replace(AUTOLINK, ' ').replace(BARE_URL, ' ');

/** Links on one line; inline code is not scanned. */
function lineLinks(text, refDefs) {
  const links = [];
  let s = text.replace(/(`+)(?:(?!\1).)+?\1/g, blank);
  const take = (pattern, make) => {
    s = s.replace(pattern, (...match) => {
      const link = make(...match);
      if (!link) return match[0];
      links.push(link);
      return blank(match[0]);
    });
  };
  take(INLINE_LINK, (m, bang, _text, dest) => ({ kind: bang ? 'image' : 'link', target: dest.replace(/^<|>$/g, '').replace(/\\(.)/g, '$1') }));
  take(REFERENCE_LINK, (m, bang, label, ref) => {
    const key = labelKey(ref || label);
    return Object.hasOwn(refDefs, key) ? { kind: bang ? 'image' : 'link', target: refDefs[key] } : { kind: 'missingReference', target: `[${ref || label}]` };
  });
  take(SHORTCUT_LINK, (m, bang, label) => {
    const key = labelKey(label);
    return Object.hasOwn(refDefs, key) ? { kind: bang ? 'image' : 'link', target: refDefs[key] } : null;
  });
  take(FOOTNOTE_REF, (m, id) => ({ kind: 'footnote', target: id }));
  take(AUTOLINK, (m, target) => ({ kind: 'link', target }));
  take(BARE_URL, (m) => {
    let url = m.replace(/[.,:;!?'"*_~]+$/, '');
    while (url.endsWith(')') && (url.match(/\(/g) ?? []).length < (url.match(/\)/g) ?? []).length) url = url.slice(0, -1);
    return { kind: 'link', target: url };
  });
  return links;
}

function remoteKey(target) {
  try {
    const url = new URL(target);
    url.hash = '';
    return url.href;
  } catch {
    return target;
  }
}

export function traceRfc(text, { file, workspace, notIds = [] }) {
  const ignore = new Set(notIds);
  const docDir = dirname(file);
  const lines = scanLines(text);
  const present = new Set(lines.filter((line) => line.kind === 'text' && /^ {0,3}##\s/.test(line.text)).map((line) => line.section));
  const missing = REQUIRED.filter((name) => ![...present].some((section) => section?.toLowerCase() === name.toLowerCase()));
  if (missing.length) throw new InputError('missingSection', `Missing H2 section(s): ${missing.join(', ')}. output.md names the required sections.`);
  for (const line of lines) {
    const match = Object.values(SECTIONS).find((name) => name.toLowerCase() === line.section?.toLowerCase());
    if (match) line.section = match;
  }
  const tables = findTables(lines);

  // Definitions.
  const defs = new Map();
  const define = (id, n) => {
    if (ignore.has(id)) return;
    if (!defs.has(id)) defs.set(id, []);
    if (!defs.get(id).includes(n)) defs.get(id).push(n);
  };
  const rowsOf = { P: [], F: [], T: [], V: [] };
  for (const table of tables) {
    for (const row of table.rows) {
      const id = exactId(row.cells[0]);
      if (!id) continue;
      const letter = id[0];
      if (TABLE_SECTION[letter]) {
        if (table.section !== TABLE_SECTION[letter]) continue;
        rowsOf[letter].push({ id, n: row.n, cells: row.cells, table });
      }
      define(id, row.n);
    }
  }
  for (const [i, line] of lines.entries()) {
    if (line.kind !== 'text' || line.table) continue;
    const label = line.text.match(LABEL_START);
    if (label && label[1][0] !== 'V') define(label[1], line.n);
    if (label && label[1][0] === 'V' && line.section === SECTIONS.validation) {
      define(label[1], line.n);
      rowsOf.V.push({ id: label[1], n: line.n, cells: [label[1], blockText(lines, i)] });
    }
    const blocking = line.text.match(BLOCKING_LABEL);
    if (blocking) for (const id of idsIn(blocking[1], ignore)) if (id[0] === 'Q') define(id, line.n);
  }
  if (!rowsOf.F.length) throw new InputError('noFeatures', `No F row in the ${SECTIONS.features} table: the trace needs the table output.md describes (F · consumer · before → after · example · accepted by).`);
  if (!rowsOf.T.length) throw new InputError('noTasks', `No T row in the ${SECTIONS.tasks} table: the trace needs the table output.md describes (T · what · where · serves · depends on · done when).`);

  // Reference definitions and footnotes, then links and ID references per line.
  const refDefs = {};
  const footnotes = new Set();
  for (const line of lines) {
    if (line.kind !== 'text') continue;
    const ref = line.text.match(REFERENCE_DEF);
    if (ref && !Object.hasOwn(refDefs, labelKey(ref[1]))) refDefs[labelKey(ref[1])] = ref[2].replace(/^<|>$/g, '');
    const note = line.text.match(FOOTNOTE_DEF);
    if (note) footnotes.add(note[1]);
  }
  const refs = new Map();
  const occurrences = [];
  let inFootnote = false;
  for (const line of lines) {
    let scanText;
    if (line.kind === 'text') {
      if (FOOTNOTE_DEF.test(line.text)) inFootnote = true;
      else if (!line.text.trim() || /^ {0,3}#/.test(line.text)) inFootnote = false;
      if (REFERENCE_DEF.test(line.text) && !FOOTNOTE_DEF.test(line.text)) continue;
      for (const link of lineLinks(line.text.replace(FOOTNOTE_DEF, blank), refDefs)) occurrences.push({ ...link, n: line.n, body: inFootnote || line.section !== SECTIONS.sources });
      scanText = idText(line.text.replace(FOOTNOTE_DEF, ''));
    } else if (line.kind === 'front' || line.kind === 'mermaid') {
      scanText = idText(line.text);
    } else continue;
    for (const id of idsIn(scanText, ignore)) {
      if (!refs.has(id)) refs.set(id, []);
      if (!refs.get(id).includes(line.n)) refs.get(id).push(line.n);
    }
  }

  const byFirstLine = (a, b) => a.lines[0] - b.lines[0] || a.id.localeCompare(b.id);
  const undefinedIds = [...refs].filter(([id]) => !defs.has(id)).map(([id, at]) => ({ id, lines: at })).sort(byFirstLine);
  const duplicateIds = [...defs].filter(([, at]) => at.length > 1).map(([id, at]) => ({ id, lines: at })).sort(byFirstLine);

  // Tasks.
  for (const table of new Set(rowsOf.T.map((row) => row.table))) {
    const needed = [['Serves', /^serves\b/], ['Depends on', /^depends on\b/]].filter(([, pattern]) => column(table.headers, pattern) < 0).map(([name]) => name);
    if (needed.length) throw new InputError('tasksTableColumns', `The ${SECTIONS.tasks} table at line ${table.n} has no ${needed.join(' or ')} column; output.md puts T · what · where · serves · depends on · done when (and state) in one table.`);
  }
  const tasks = [];
  for (const row of rowsOf.T) {
    if (tasks.some((task) => task.id === row.id)) continue;
    const { headers } = row.table;
    const cell = (pattern, fallback = -1) => {
      const at = column(headers, pattern);
      return row.cells[at >= 0 ? at : fallback] ?? '';
    };
    tasks.push({
      id: row.id,
      n: row.n,
      serves: idsIn(cell(/^serves\b/), ignore).filter((id) => /^[FP]/.test(id) && defs.has(id)),
      deps: idsIn(cell(/^depends on\b/), ignore).filter((id) => id[0] === 'T'),
      what: plain(cell(/^(make|what)\b/, 1)),
      state: plain(cell(/^state$/)),
    });
  }
  const order = new Map(tasks.map((task, index) => [task.id, index]));
  const depsOf = (task) => task.deps.filter((id) => order.has(id));
  const tasksWithoutServes = tasks.filter((task) => !task.serves.length).map((task) => task.id);
  const forwardDeps = tasks.flatMap((task) => depsOf(task).filter((id) => order.get(id) > order.get(task.id)).map((id) => ({ task: task.id, dependsOn: id, line: task.n })));
  const cycles = stronglyConnected(tasks, depsOf, order);
  const criticalPath = cycles.length ? [] : longestChain(tasks, depsOf, order);

  // Features against tasks and checks.
  const served = new Set(tasks.flatMap((task) => task.serves));
  const checked = new Set(rowsOf.V.flatMap((row) => idsIn(row.cells.slice(1).join(' | '), ignore)));
  const planned = [];
  for (const row of rowsOf.F) {
    if (planned.includes(row.id)) continue;
    const priority = column(row.table.headers, /^priority$/);
    if (priority >= 0 && /^won[’']?t\b/i.test(plain(row.cells[priority] ?? ''))) continue;
    planned.push(row.id);
  }
  const featuresWithoutTask = planned.filter((id) => !served.has(id));
  const featuresWithoutCheck = planned.filter((id) => !checked.has(id));

  // Citations and local links: the report renderer's resolution, then the plain path when it has none.
  const anchors = headingAnchors(lines);
  const resolveLocal = (target) => {
    const found = resolveRefs(`\`${target}\``, docDir, workspace)[target];
    if (found) return { abs: found.abs, line: found.line };
    const hash = target.indexOf('#');
    const fragment = hash >= 0 ? target.slice(hash + 1) : '';
    const path = decode(hash >= 0 ? target.slice(0, hash) : target);
    const abs = [docDir, workspace].map((base) => resolve(base, path)).find((candidate) => existsSync(candidate));
    if (!abs) return { abs: resolve(docDir, path), reason: 'notFound' };
    return { abs, reason: fragment && !/^L?\d+(?:[-–:]L?\d+)?$/.test(fragment) ? 'anchorNotFound' : null };
  };
  const sourceKeys = new Set();
  const citations = new Map();
  const brokenLocalLinks = [];
  const broken = (link, reason) => brokenLocalLinks.push({ target: link.target, line: link.n, reason });
  for (const link of occurrences) {
    if (link.kind === 'missingReference') { broken(link, 'referenceUndefined'); continue; }
    if (link.kind === 'footnote') { if (!footnotes.has(link.target)) broken(link, 'footnoteUndefined'); continue; }
    const target = link.target.trim();
    if (!target) continue;
    let key;
    if (target.startsWith('#')) {
      const fragment = decode(target.slice(1));
      if (!/^fn(ref)?-/.test(fragment) && !anchors.has(kit.slugify(fragment))) broken(link, 'anchorNotFound');
      continue;
    }
    if (/^[a-z][\w+.-]*:/i.test(target)) {
      if (!/^https?:/i.test(target)) continue;
      key = remoteKey(target);
    } else {
      const resolved = resolveLocal(target);
      key = resolved.abs;
      if (resolved.reason) broken(link, resolved.reason);
      else if (resolved.line && statSync(resolved.abs).isFile() && resolved.line > readFileSync(resolved.abs, 'utf8').split(/\r?\n/).length) broken(link, 'lineBeyondEnd');
    }
    if (link.kind === 'image') continue;
    if (!link.body) { sourceKeys.add(key); continue; }
    if (!citations.has(key)) citations.set(key, { target, lines: [] });
    if (!citations.get(key).lines.includes(link.n)) citations.get(key).lines.push(link.n);
  }
  const citationsNotInSources = [...citations].filter(([key]) => !sourceKeys.has(key)).map(([, value]) => value);

  const findings = { undefinedIds, duplicateIds, featuresWithoutTask, featuresWithoutCheck, tasksWithoutServes, forwardDeps, cycles, citationsNotInSources, brokenLocalLinks };
  return {
    ok: Object.values(findings).every((list) => !list.length),
    file,
    workspace,
    undefinedIds,
    duplicateIds,
    featuresWithoutTask,
    featuresWithoutCheck,
    tasksWithoutServes,
    forwardDeps,
    cycles,
    criticalPath,
    taskGraphBranches: criticalPath.length !== tasks.length,
    citationsNotInSources,
    brokenLocalLinks,
    taskGraphMermaid: taskGraph(tasks, depsOf, criticalPath),
  };
}

/** A labelled list item or paragraph with its continuation lines (wrapped text, nested items). */
function blockText(lines, start) {
  const indent = (lines[start].text.match(LIST_ITEM)?.[1] ?? lines[start].text.match(/^\s*/)[0]).length;
  const parts = [lines[start].text];
  for (let i = start + 1; i < lines.length; i++) {
    const { kind, text, table } = lines[i];
    if (kind !== 'text' || table || !text.trim() || /^ {0,3}#/.test(text) || LABEL_START.test(text)) break;
    const item = text.match(LIST_ITEM);
    if (item && item[1].length <= indent) break;
    parts.push(text);
  }
  return parts.join(' ');
}

/** Heading anchors as the report renderer assigns them: slug of the heading text, -1, -2 for repeats. */
function headingAnchors(lines) {
  const anchors = new Set();
  const seen = Object.create(null);
  for (let i = 0; i < lines.length; i++) {
    const { kind, text } = lines[i];
    if (kind !== 'text') continue;
    const next = lines[i + 1];
    const heading = text.match(/^ {0,3}#{1,6}\s+(.+?)\s*#*\s*$/)?.[1]
      ?? (text.trim() && next?.kind === 'text' && /^ {0,3}(=+|-+)\s*$/.test(next.text) && !/^\s*[-*+|>]/.test(text) ? text.trim() : null);
    if (!heading) continue;
    const slug = kit.slugify(heading.replace(/\[([^\]]+)\]\([^)]*\)/g, '$1'));
    const count = seen[slug] ?? 0;
    seen[slug] = count + 1;
    anchors.add(count ? `${slug}-${count}` : slug);
  }
  return anchors;
}

/** Tarjan's strongly connected components with more than one task, or a task that depends on itself. */
function stronglyConnected(tasks, depsOf, order) {
  const byId = new Map(tasks.map((task) => [task.id, task]));
  const index = new Map();
  const low = new Map();
  const stack = [];
  const onStack = new Set();
  const cycles = [];
  let counter = 0;
  const visit = (id) => {
    index.set(id, counter);
    low.set(id, counter++);
    stack.push(id);
    onStack.add(id);
    for (const dep of depsOf(byId.get(id))) {
      if (!index.has(dep)) {
        visit(dep);
        low.set(id, Math.min(low.get(id), low.get(dep)));
      } else if (onStack.has(dep)) low.set(id, Math.min(low.get(id), index.get(dep)));
    }
    if (low.get(id) !== index.get(id)) return;
    const component = [];
    let member;
    do {
      member = stack.pop();
      onStack.delete(member);
      component.push(member);
    } while (member !== id);
    if (component.length > 1 || depsOf(byId.get(id)).includes(id)) cycles.push(component.sort((a, b) => order.get(a) - order.get(b)));
  };
  for (const task of tasks) if (!index.has(task.id)) visit(task.id);
  return cycles.sort((a, b) => order.get(a[0]) - order.get(b[0]));
}

/** The longest dependency chain by task count; ties go to the task listed first. */
function longestChain(tasks, depsOf, order) {
  const byId = new Map(tasks.map((task) => [task.id, task]));
  const best = new Map();
  const visit = (id) => {
    if (best.has(id)) return best.get(id);
    let result = { length: 1, prev: null };
    for (const dep of depsOf(byId.get(id)).toSorted((a, b) => order.get(a) - order.get(b))) {
      const chain = visit(dep);
      if (chain.length + 1 > result.length) result = { length: chain.length + 1, prev: dep };
    }
    best.set(id, result);
    return result;
  };
  let end = null;
  for (const task of tasks) if (visit(task.id).length > (end ? best.get(end).length : 0)) end = task.id;
  const path = [];
  for (let id = end; id; id = best.get(id).prev) path.unshift(id);
  return path;
}

function taskGraph(tasks, depsOf, criticalPath) {
  const escape = (text) => text.replace(/"/g, '#quot;').replace(/</g, '#lt;').replace(/>/g, '#gt;');
  const critical = new Set(criticalPath.slice(1).map((id, i) => `${criticalPath[i]}>${id}`));
  const out = ['flowchart LR'];
  for (const task of tasks) {
    const word = task.state.toLowerCase().match(/^[a-z]+/)?.[0];
    const state = STATES.has(word) && word !== 'todo' ? `${word}: ` : '';
    const questions = word === 'blocked' ? idsIn(task.state).filter((id) => id[0] === 'Q') : [];
    const label = [`${state}${task.id}`, task.what].filter(Boolean).join(' ');
    out.push(`  ${task.id}["${escape([label, ...questions].join(', '))}"]`);
  }
  for (const task of tasks) {
    for (const dep of depsOf(task)) out.push(critical.has(`${dep}>${task.id}`) ? `  ${dep} ==>|critical| ${task.id}` : `  ${dep} --> ${task.id}`);
  }
  return out.join('\n');
}

function parseArgs(args) {
  const options = { files: [], notIds: [], mermaid: false, workspace: null, help: false };
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--help' || arg === '-h') options.help = true;
    else if (arg === '--mermaid') options.mermaid = true;
    else if (arg === '--workspace' || arg === '--not-id') {
      const value = args[++i];
      if (value === undefined || value.startsWith('--')) throw new InputError('invalidArguments', `Missing value for ${arg}`);
      if (arg === '--workspace') options.workspace = resolve(value);
      else {
        for (const id of value.split(',').map((item) => item.trim()).filter(Boolean)) {
          if (!ID_SHAPE.test(id)) throw new InputError('invalidArguments', `--not-id takes P/F/D/T/V/Q tokens such as P99, got ${id}`);
          options.notIds.push(id);
        }
      }
    } else if (arg.startsWith('-')) throw new InputError('invalidArguments', `Unknown option: ${arg}`);
    else options.files.push(arg);
  }
  if (!options.help && options.files.length !== 1) throw new InputError('invalidArguments', 'Pass exactly one RFC Markdown file; see --help.');
  return options;
}

function main() {
  let options;
  let report;
  try {
    options = parseArgs(process.argv.slice(2));
    if (options.help) {
      console.log(HELP);
      return;
    }
    const file = resolve(options.files[0]);
    if (!existsSync(file) || !statSync(file).isFile()) throw new InputError('fileNotFound', `Not a file: ${file}`);
    const workspace = options.workspace ?? collect(file).workspace;
    if (!existsSync(workspace) || !statSync(workspace).isDirectory()) throw new InputError('invalidArguments', `Not a folder: ${workspace}`);
    report = traceRfc(readFileSync(file, 'utf8'), { file, workspace, notIds: options.notIds });
  } catch (error) {
    if (error instanceof InputError) fail(error.code, error.message, 2);
    fail('traceFailed', errorMessage(error), 1);
  }
  if (options.mermaid) {
    console.log(`\`\`\`mermaid\n${report.taskGraphMermaid}\n\`\`\``);
    return;
  }
  console.log(JSON.stringify(report, null, 2));
  process.exitCode = report.ok ? 0 : 1;
}

if (isMainModule(import.meta.url)) main();
