import fs from 'node:fs';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { archiveFolder, ownArchives, RECORD_FILE, setSessionLineage, writeSegment, type Segment } from '../src/shared/archive.js';
import { writeSessionRecord, type SessionRecord } from '../src/shared/session-record.js';
import { PARENT_SESSION_ENV } from '../src/shared/env.js';
import { historyFiles, historyScope, listHistory, PAGE_CHARS, READ_LINES, readableHistory, readableSessionFolder, readHistory, searchHistory } from '../src/shared/history.js';
import { sessionFolder, setCurrentSession } from '../src/shared/home.js';
import { repo, tmp } from './helpers.js';

const segment = (session: string, tldr: string): Omit<Segment, 'n'> => ({ session, entry: `e-${session}`, tokensBefore: 1_000, method: 'cache-shared', tldr, topics: [], readFiles: [], modifiedFiles: [], messages: 1 });

const record = (session: string, cwd: string, overrides: Partial<SessionRecord> = {}): SessionRecord => ({
  session, repository: fs.realpathSync(cwd), workspace: cwd, goal: { role: 'user', text: `goal of ${session}` }, ending: [{ role: 'assistant', text: `${session} ended well` }], toolCalls: 0, ...overrides,
});

/** Files `body` as a segment of `session`, and its record when `cwd` is given (`writeSegment` files under the current session). */
function session(id: string, options: { body?: string; tldr?: string; cwd?: string; record?: Partial<SessionRecord> } = {}): void {
  setCurrentSession(id);
  if (options.body !== undefined) writeSegment(segment(id, options.tldr ?? `work of ${id}`), options.body);
  if (options.cwd) writeSessionRecord(record(id, options.cwd, options.record));
  setCurrentSession('me');
}

let saved: string | undefined;
beforeEach(() => {
  saved = process.env['OCTOCODE_HOME'];
  process.env['OCTOCODE_HOME'] = tmp();
  setCurrentSession('me');
});
afterEach(() => {
  setCurrentSession(undefined);
  setSessionLineage([]);
  vi.unstubAllEnvs();
  if (saved === undefined) delete process.env['OCTOCODE_HOME'];
  else process.env['OCTOCODE_HOME'] = saved;
});

describe('history scope', () => {
  it('is this session, its fork sources, its parent, then the repository\'s sessions by last activity; never another repository\'s', () => {
    const cwd = repo();
    const elsewhere = repo();
    session('me', { body: 'mine', cwd });
    session('source', { body: 'before the fork' });
    session('boss', { body: 'parent work' });
    session('older', { cwd, record: { lastActive: '2026-01-01T00:00:00Z' } });
    session('newer', { body: 'sibling work', cwd, record: { lastActive: '2026-02-01T00:00:00Z' } });
    session('far', { body: 'secret', cwd: elsewhere });
    session('empty', {});
    setSessionLineage(['source']);
    vi.stubEnv(PARENT_SESSION_ENV, 'boss');
    const scope = historyScope(cwd);
    expect(scope.map((entry) => [entry.id, entry.relation])).toEqual([
      ['me', 'this session'], ['source', 'forked from'], ['boss', 'parent session'], ['newer', 'same repository'], ['older', 'same repository'],
    ]);
    expect(scope[0]!.record).toMatchObject({ session: 'me', goal: 'goal of me', ended: 'me ended well' });
    // The record comes first, then segments newest first.
    expect(historyFiles(scope.slice(0, 1)).map((file) => path.basename(file.file))).toEqual([RECORD_FILE, expect.stringMatching(/^1-/)]);
  });

  it('makes a fork source\'s archive this session\'s own history (B155)', () => {
    session('source', { body: 'before the fork' });
    expect(ownArchives()).not.toContain(archiveFolder('source'));
    setSessionLineage(['source']);
    expect(ownArchives()).toContain(archiveFolder('source'));
  });
});

describe('what reads without asking', () => {
  it('is history of this session\'s own and of its repository\'s sessions, not another repository\'s, nor other session files', () => {
    const cwd = repo();
    session('me', { body: 'mine' });
    session('sib', { body: 'sibling', cwd });
    session('far', { body: 'secret', cwd: repo() });
    session('source', { body: 'forked' });
    setSessionLineage(['source']);
    for (const id of ['me', 'sib', 'source']) expect(readableHistory(path.join(archiveFolder(id), RECORD_FILE), cwd), id).toBe(true);
    expect(readableHistory(path.join(archiveFolder('far'), RECORD_FILE), cwd)).toBe(false);
    expect(readableHistory(path.join(sessionFolder('output'), 'x.txt'), cwd)).toBe(false);
    expect(readableHistory(path.join(cwd, 'a.ts'), cwd)).toBe(false);
    // MCP research tools decide per session folder: a session's own output reads freely, another repository's does not.
    expect(readableSessionFolder(path.join(sessionFolder('output'), 'x.txt'), cwd)).toBe(true);
    expect(readableSessionFolder(path.join(sessionFolder('output', 'sib'), 'x.txt'), cwd)).toBe(true);
    expect(readableSessionFolder(path.join(sessionFolder('output', 'far'), 'x.txt'), cwd)).toBe(false);
    expect(readableSessionFolder(path.dirname(path.dirname(archiveFolder())), cwd)).toBe(false);
  });
});

describe('search and read', () => {
  it('shows every hit with context, marks it, names the file and the session, shows a long hit line whole and points to a long context line', () => {
    const cwd = repo();
    session('me', { body: ['alpha', 'one', 'two', 'needle here', 'three', `${'x'.repeat(600)} needle far`, 'end', 'z'.repeat(900), 'four', 'five', 'six', 'last needle'].join('\n'), cwd });
    const files = historyFiles(historyScope(cwd));
    const found = searchHistory(files, /needle/i);
    expect(found).toMatchObject({ hits: 3, files: 1, page: 1, pages: 1 });
    expect(found.text).toMatch(/^## .*1-e-me\.md · me \(this session\) · segment 1 · work of me$/m);
    expect(found.text).toMatch(/^> +\d+│ needle here$/m);
    expect(found.text).toMatch(/^ +\d+│ two$/m);
    expect(found.text).toMatch(new RegExp(`^> +\\d+│ ${'x'.repeat(600)} needle far$`, 'm'));
    expect(found.text).toMatch(/^ +25│ \[a context line of 900 characters, not a hit: read it with file and line 25\]$/m);
    expect(found.text).not.toContain('z'.repeat(10));
    expect(found.text).not.toContain('cut');
  });

  it('pages a large result so every hit is reached exactly once', () => {
    const cwd = repo();
    const body = Array.from({ length: 2_000 }, (_, index) => `line ${index} hit ${'y'.repeat(40)}`).join('\n');
    session('me', { body, cwd });
    const files = historyFiles(historyScope(cwd));
    const first = searchHistory(files, /hit/);
    expect(first.pages).toBeGreaterThan(1);
    const seen: string[] = [];
    for (let page = 1; page <= first.pages; page++) {
      const text = searchHistory(files, /hit/, page).text;
      expect(text.length).toBeLessThanOrEqual(PAGE_CHARS + 200);
      seen.push(...text.split('\n').filter((line) => line.startsWith('>')).map((line) => line.replace(/^>\s*(\d+)│.*$/, '$1')));
    }
    expect(new Set(seen).size).toBe(seen.length);
    expect(seen.length).toBe(first.hits);
  });

  it('reads a window of whole lines and says where the next one starts', () => {
    const cwd = repo();
    session('me', { body: Array.from({ length: 300 }, (_, index) => `row ${index}`).join('\n'), cwd });
    const file = historyFiles(historyScope(cwd)).find((entry) => entry.label.startsWith('segment'))!.file;
    const read = readHistory(file, 10)!;
    expect(read).toMatchObject({ first: 10, last: 10 + READ_LINES - 1, page: 1, pages: 1 });
    expect(read.text.split('\n')[0]).toMatch(/^ +10│ /);
    expect(readHistory(path.join(archiveFolder(), 'missing.md'))).toBeUndefined();
  });

  it('lists each session with its record labels and files', () => {
    const cwd = repo();
    session('me', { body: 'mine', tldr: 'did the thing', cwd, record: { lastActive: '2026-03-01T00:00:00Z' } });
    const listed = listHistory(historyScope(cwd));
    expect(listed.text).toContain(`## me (this session) · ${archiveFolder()}`);
    expect(listed.text).toContain('last active 2026-03-01T00:00:00Z · goal: goal of me · ended: me ended well');
    expect(listed.text).toContain(`- ${RECORD_FILE} · record: goal and ending`);
    expect(listed.text).toMatch(/- 1-e-me\.md · segment 1 · did the thing/);
  });
});
