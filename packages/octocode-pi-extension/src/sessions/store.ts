import { execFile } from 'node:child_process';
import type { AgentDb } from '../agentdb/db.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';

/**
 * What Octocode adds to one Pi session (`sessions` row): the git branch and HEAD it last saw, its cost and tokens, the
 * files it changed, and the pid of the process that has it open. Everything else (file, cwd, name, first message,
 * times) comes from Pi's session listing.
 */
export interface SessionExtra {
  id: string;
  branch: string | null;
  head: string | null;
  cost: number;
  tokens: number;
  files_changed: number;
  pid: number | null;
}

interface SessionStats {
  cost: number;
  tokens: number;
  filesChanged: number;
  branch?: string;
  head?: string;
}

/** One line of text as shown: terminal controls stripped, whitespace collapsed, at most `max` characters. */
export function oneLine(text: string, max: number): string {
  // By code point, so a cut never splits a surrogate pair (an emoji).
  const line = [...sanitizeTerminalText(text).replace(/\s+/g, ' ').trim()];
  return line.length > max ? `${line.slice(0, max - 1).join('')}…` : line.join('');
}

/** Runs git off the event loop; rejects on failure or after 2 s. */
const git = (cwd: string, args: string[]) =>
  new Promise<string>((resolve, reject) => {
    execFile('git', args, { cwd, encoding: 'utf8', timeout: 2000, windowsHide: true }, (error, stdout) => (error ? reject(error) : resolve(stdout)));
  });

/**
 * The checked-out branch of `cwd` (`HEAD` short name; undefined on a detached HEAD) and the HEAD commit; both undefined
 * outside git or before the first commit. Never rejects.
 */
export async function gitState(cwd: string): Promise<{ branch?: string; head?: string }> {
  try {
    const [head = '', branch = ''] = (await git(cwd, ['rev-parse', 'HEAD', '--abbrev-ref', 'HEAD'])).trim().split('\n');
    return { ...(branch && branch !== 'HEAD' ? { branch: oneLine(branch, 120) } : {}), ...(/^[0-9a-f]{7,64}$/.test(head) ? { head } : {}) };
  } catch {
    return {};
  }
}

/** Changed or untracked paths in the working tree of `cwd` (0 outside git or when git is slow). Never rejects. */
export async function gitDirty(cwd: string): Promise<number> {
  try {
    return (await git(cwd, ['status', '--porcelain'])).split('\n').filter(Boolean).length;
  } catch {
    return 0;
  }
}

/** Commits from `from` to `to`, or undefined when git cannot tell (e.g. `from` was rewritten away). Never rejects. */
export async function gitCommitsBetween(cwd: string, from: string, to: string): Promise<number | undefined> {
  try {
    return Number((await git(cwd, ['rev-list', '--count', `${from}..${to}`])).trim());
  } catch {
    return undefined;
  }
}

/** The session extras: one row per Pi session this extension has run. */
export class SessionIndex {
  constructor(readonly agent: AgentDb) {}

  get(id: string): SessionExtra | undefined {
    return this.agent.db.prepare('SELECT * FROM sessions WHERE id = ?').get(id) as SessionExtra | undefined;
  }

  /** Extras of the given sessions, by id. */
  extras(ids: readonly string[]): Map<string, SessionExtra> {
    const rows = new Map<string, SessionExtra>();
    const select = this.agent.db.prepare('SELECT * FROM sessions WHERE id = ?');
    for (const id of ids) {
      const row = select.get(id) as SessionExtra | undefined;
      if (row) rows.set(id, row);
    }
    return rows;
  }

  /** This process has the session open now. Keeps what earlier runs recorded. */
  start(id: string): void {
    this.agent.db.prepare('INSERT INTO sessions (id, pid) VALUES (?, ?) ON CONFLICT(id) DO UPDATE SET pid = excluded.pid').run(id, process.pid);
  }

  stats(id: string, stats: SessionStats): void {
    this.agent.db
      .prepare(
        `INSERT INTO sessions (id, cost, tokens, files_changed, branch, head, pid) VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET cost = excluded.cost, tokens = excluded.tokens, files_changed = excluded.files_changed,
           branch = COALESCE(excluded.branch, sessions.branch), head = COALESCE(excluded.head, sessions.head)`,
      )
      .run(id, stats.cost, stats.tokens, stats.filesChanged, stats.branch ?? null, stats.head ?? null, process.pid);
  }

  /** Records where git stood, without touching the stats. */
  git(id: string, state: { branch?: string; head?: string }): void {
    this.agent.db
      .prepare('UPDATE sessions SET branch = COALESCE(?, branch), head = COALESCE(?, head) WHERE id = ?')
      .run(state.branch ?? null, state.head ?? null, id);
  }

  /** The session's process has ended: it is no longer live. */
  end(id: string): void {
    this.agent.db.prepare('UPDATE sessions SET pid = NULL WHERE id = ? AND pid = ?').run(id, process.pid);
  }

  all(): SessionExtra[] {
    return this.agent.db.prepare('SELECT * FROM sessions').all() as unknown as SessionExtra[];
  }

  forget(id: string): void {
    this.agent.db.prepare('DELETE FROM sessions WHERE id = ?').run(id);
  }

  meta(key: string): string | undefined {
    return (this.agent.db.prepare('SELECT value FROM meta WHERE key = ?').get(key) as { value: string } | undefined)?.value;
  }

  setMeta(key: string, value: string): void {
    this.agent.db.prepare('INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value').run(key, value);
  }
}
