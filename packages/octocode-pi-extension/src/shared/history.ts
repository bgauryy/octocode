import fs from 'node:fs';
import path from 'node:path';
import { archiveFolder, ownArchives, readSegments, RECORD_FILE, segmentFiles, sessionLineage, type Segment } from './archive.js';
import { readRecordHeader, type RecordHeader } from './session-record.js';
import { PARENT_SESSION_ENV } from './env.js';
import { canonicalPath, isWithin, repoKey, sessionsRoot } from './home.js';

/**
 * The history a session may read: archives of compacted messages and session records (`shared/archive.ts`). Its own
 * archive, those of the sessions it was forked from and, in a subagent, its parent's are its own history; the others of
 * the same repository (their record names it) are its repository's history. Each of those reads without asking
 * (`readableHistory`); another repository's asks. The `archive` tool searches and reads them here.
 */

type Relation = 'this session' | 'forked from' | 'parent session' | 'same repository';

interface HistorySession {
  /** The session's folder name (its id as a folder name). */
  id: string;
  /** Its archive folder. */
  folder: string;
  relation: Relation;
  record?: RecordHeader;
  segments: Segment[];
}

/** The session folder and archive folder `file` is in, when it is inside an archive under the sessions root. */
function archiveOwner(file: string, env: NodeJS.ProcessEnv): { id: string; folder: string } | undefined {
  const root = canonicalPath(sessionsRoot(env));
  const real = canonicalPath(file);
  if (!isWithin(real, root)) return undefined;
  const [id, kind] = path.relative(root, real).split(path.sep);
  return id && kind === 'archive' ? { id, folder: path.join(root, id, 'archive') } : undefined;
}

/**
 * Whether `file` is in an archive this session reads without asking: its own history (`ownArchives`), or a session's
 * whose record names the repository of `cwd`. Any other path, another repository's archive included, is not.
 */
export function readableHistory(file: string, cwd: string, env: NodeJS.ProcessEnv = process.env): boolean {
  const owner = archiveOwner(file, env);
  if (!owner) return false;
  if (ownArchives(env).some((dir) => canonicalPath(dir) === owner.folder)) return true;
  return readRecordHeader(owner.folder)?.repository === repoKey(cwd).key;
}

/**
 * Whether `file` is in a session folder whose data this session reads without asking: one of its own history's (its
 * own, its forks' sources, its parent's) or one of the same repository. The sessions root itself, and another
 * repository's session, are not.
 */
export function readableSessionFolder(file: string, cwd: string, env: NodeJS.ProcessEnv = process.env): boolean {
  const root = canonicalPath(sessionsRoot(env));
  const real = canonicalPath(file);
  if (!isWithin(real, root)) return false;
  const [id] = path.relative(root, real).split(path.sep);
  if (!id) return false;
  if (ownArchives(env).some((dir) => path.basename(path.dirname(dir)) === id)) return true;
  return readRecordHeader(path.join(root, id, 'archive'))?.repository === repoKey(cwd).key;
}

/**
 * The sessions with history this session may read, in the order they are searched: this one, those it was forked from
 * (nearest first), its parent session, then the repository's other sessions, most recently active first. A session
 * with neither a record nor a segment is left out.
 */
export function historyScope(cwd: string, env: NodeJS.ProcessEnv = process.env): HistorySession[] {
  const seen = new Set<string>();
  const sessions: HistorySession[] = [];
  const add = (folder: string, relation: Relation, record = readRecordHeader(folder)) => {
    const id = path.basename(path.dirname(folder));
    if (seen.has(id)) return;
    seen.add(id);
    const segments = readSegments(id, env);
    if (record || segments.length > 0) sessions.push({ id, folder, relation, ...(record ? { record } : {}), segments });
  };
  const byId = (id: string | undefined, relation: Relation) => {
    if (!id) return;
    try {
      add(archiveFolder(id, env), relation);
    } catch {
      // An id that cannot name a folder has no history.
    }
  };
  add(archiveFolder(undefined, env), 'this session');
  for (const id of sessionLineage()) byId(id, 'forked from');
  byId(env[PARENT_SESSION_ENV]?.trim(), 'parent session');
  const key = repoKey(cwd).key;
  const root = sessionsRoot(env);
  let names: string[] = [];
  try {
    names = fs.readdirSync(root);
  } catch {
    // No sessions folder: no other history.
  }
  const others = names.flatMap((name) => {
    if (seen.has(name)) return [];
    const folder = path.join(root, name, 'archive');
    const record = readRecordHeader(folder);
    return record?.repository === key ? [{ folder, record }] : [];
  });
  others.sort((a, b) => (b.record.lastActive ?? '').localeCompare(a.record.lastActive ?? ''));
  for (const { folder, record } of others) add(folder, 'same repository', record);
  return sessions;
}

/** One file of a session's history: its record, or a segment. */
export interface HistoryFile {
  file: string;
  session: HistorySession;
  /** `record`, or `segment <n> · <TL;DR>`. */
  label: string;
}

/** The files of `sessions` in search order: each session's record, then its segments newest first. */
export function historyFiles(sessions: readonly HistorySession[]): HistoryFile[] {
  return sessions.flatMap((session) => {
    const tldr = new Map(session.segments.map((segment) => [segment.n, segment.tldr]));
    const record = session.record ? [{ file: path.join(session.folder, RECORD_FILE), session, label: 'record: goal and ending' }] : [];
    const segments = segmentFiles(session.folder).reverse().map(({ n, name }) => ({ file: path.join(session.folder, name), session, label: `segment ${n}${tldr.get(n) ? ` · ${tldr.get(n)}` : ''}` }));
    return [...record, ...segments];
  });
}

/** The files of `files` that are `target` or inside it (a session's archive folder); none when it names none of them. */
export function filesUnder(files: readonly HistoryFile[], target: string): HistoryFile[] {
  const real = canonicalPath(target);
  return files.filter((entry) => {
    const file = canonicalPath(entry.file);
    return file === real || isWithin(file, real);
  });
}

/** Lines shown around each hit. */
const HIT_CONTEXT = 4;
/**
 * A context line (not a hit) longer than this is shown as a pointer to read it with `file` and `line`, never as a part of
 * it. A hit line is always whole, split across pages when it is longer than one.
 */
const CONTEXT_LINE_CHARS = 400;
/** Characters of one page of results or of a read. */
export const PAGE_CHARS = 12_000;
/** Lines one read shows (then it continues by `line`). */
export const READ_LINES = 120;

const lineLabel = (n: number, hit: boolean) => `${hit ? '>' : ' '}${String(n).padStart(6)}│ `;

const contextLine = (line: string, n: number): string =>
  line.length <= CONTEXT_LINE_CHARS ? line : `[a context line of ${line.length} characters, not a hit: read it with file and line ${n}]`;

/** `lines` packed into pages of at most `PAGE_CHARS` (a longer line split across pages); each page's text, in order. */
function paginate(blocks: ReadonlyArray<{ heading?: string; lines: string[] }>): string[] {
  const pages: string[][] = [[]];
  let size = 0;
  const push = (text: string) => {
    pages.at(-1)!.push(text);
    size += text.length + 1;
  };
  for (const block of blocks) {
    const continued = block.heading ? `${block.heading} (continued)` : undefined;
    if (block.heading) {
      if (size > 0 && size + block.heading.length > PAGE_CHARS) {
        pages.push([]);
        size = 0;
      }
      push(block.heading);
    }
    for (const line of block.lines) {
      for (let at = 0; at === 0 || at < line.length; at += PAGE_CHARS) {
        const part = line.slice(at, at + PAGE_CHARS);
        if (size > 0 && size + part.length > PAGE_CHARS) {
          pages.push([]);
          size = 0;
          if (continued) push(continued);
        }
        push(part);
      }
    }
  }
  return pages.map((page) => page.join('\n'));
}

interface HistoryPage {
  text: string;
  page: number;
  pages: number;
}

interface SearchResult extends HistoryPage {
  hits: number;
  files: number;
}

const readLines = (file: string): string[] | undefined => {
  try {
    return fs.readFileSync(file, 'utf8').split('\n');
  } catch {
    return undefined;
  }
};

/** The sessions in scope, each with its record's labels and its files (record, then segments newest first), paged. */
export function listHistory(sessions: readonly HistorySession[], page = 1): HistoryPage {
  const files = historyFiles(sessions);
  const blocks = sessions.map((session) => {
    const record = session.record;
    const facts = [
      ...(record?.lastActive ? [`last active ${record.lastActive}`] : []),
      ...(record?.name ? [`name ${record.name}`] : []),
      ...(record?.goal ? [`goal: ${record.goal}`] : []),
      ...(record?.ended ? [`ended: ${record.ended}`] : []),
    ];
    return {
      heading: `## ${session.id} (${session.relation}) · ${session.folder}`,
      lines: [...(facts.length > 0 ? [facts.join(' · ')] : []), ...files.filter((file) => file.session === session).map((file) => `- ${path.basename(file.file)} · ${file.label}`)],
    };
  });
  const pages = paginate(blocks);
  const index = Math.min(Math.max(page, 1), pages.length);
  return { text: pages[index - 1] ?? '', page: index, pages: pages.length };
}

/** Every line of `files` that `pattern` matches, with `HIT_CONTEXT` lines around it, paged; `page` is 1-based. */
export function searchHistory(files: readonly HistoryFile[], pattern: RegExp, page = 1): SearchResult {
  const test = new RegExp(pattern.source, pattern.flags.replace('g', ''));
  const blocks: Array<{ heading: string; lines: string[] }> = [];
  let hits = 0;
  for (const { file, session, label } of files) {
    const lines = readLines(file);
    if (!lines) continue;
    const matched = lines.flatMap((line, index) => (test.test(line) ? [index] : []));
    if (matched.length === 0) continue;
    hits += matched.length;
    const hit = new Set(matched);
    const shown: string[] = [];
    let end = -1;
    for (const index of matched) {
      const from = Math.max(index - HIT_CONTEXT, end + 1);
      if (end >= 0 && from > end + 1) shown.push('     …');
      for (let at = from; at <= Math.min(index + HIT_CONTEXT, lines.length - 1); at++) shown.push(`${lineLabel(at + 1, hit.has(at))}${hit.has(at) ? lines[at]! : contextLine(lines[at]!, at + 1)}`);
      end = Math.max(end, Math.min(index + HIT_CONTEXT, lines.length - 1));
    }
    blocks.push({ heading: `## ${file} · ${session.id} (${session.relation}) · ${label}`, lines: shown });
  }
  const pages = paginate(blocks);
  const index = Math.min(Math.max(page, 1), pages.length);
  return { text: pages[index - 1] ?? '', page: index, pages: blocks.length === 0 ? 0 : pages.length, hits, files: blocks.length };
}

interface ReadResult extends HistoryPage {
  first: number;
  last: number;
  total: number;
}

/** Lines `line` to `line + READ_LINES - 1` of `file`, whole, paged; undefined when it cannot be read. */
export function readHistory(file: string, line = 1, page = 1): ReadResult | undefined {
  const lines = readLines(file);
  if (!lines) return undefined;
  const total = lines.length;
  const first = Math.min(Math.max(line, 1), total);
  const last = Math.min(first + READ_LINES - 1, total);
  const pages = paginate([{ lines: lines.slice(first - 1, last).map((text, index) => `${lineLabel(first + index, false)}${text}`) }]);
  const index = Math.min(Math.max(page, 1), pages.length);
  return { text: pages[index - 1] ?? '', page: index, pages: pages.length, first, last, total };
}
