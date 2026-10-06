import type { AgentDb } from '../agentdb/db.js';
import { processAlive } from '../shared/process.js';
import { sanitizeTerminalText, secretProblem, storedText } from '../shared/sanitize.js';

/**
 * The per-repository backlog in the shared agent database (`backlog`, `backlog_notes`; schema in src/agentdb). Items are
 * numbered per repository (`B12`); each call is one transaction, so several Pi processes can work the same board.
 *
 *   backlog   triage inbox: everything an agent proposes lands here
 *   todo      accepted, not started
 *   ongoing   claimed by one session or subagent (the assignee)
 *   done      finished, with a note saying what changed and how it was verified
 */

export const STATES = ['backlog', 'todo', 'ongoing', 'done'] as const;
export type State = (typeof STATES)[number];
export const PRIORITIES = ['p0', 'p1', 'p2', 'p3'] as const;
export type Priority = (typeof PRIORITIES)[number];

export const TITLE_MAX = 200;
export const BODY_MAX = 4000;
export const NOTE_MAX = 1000;
export const TAGS_MAX = 8;

export interface Item {
  /** Database row id (notes hang from it). */
  rowId: number;
  seq: number;
  /** `B<seq>`. */
  ref: string;
  title: string;
  body: string;
  state: State;
  priority: number;
  tags: string[];
  assignee?: string;
  createdBy: string;
  sourceSession?: string;
  createdAt: number;
  updatedAt: number;
  doneAt?: number;
  version: number;
}

export interface Note {
  at: number;
  author: string;
  text: string;
}

/** Who is acting: the id written as assignee and note author, and for a subagent the sessions of its parent. */
export interface Actor {
  id: string;
  /** Set for a subagent: its parent's sessions, the only assignees whose items it may update (it never claims). */
  owners?: ReadonlySet<string>;
}

export class BacklogError extends Error {}

interface Row {
  id: number;
  seq: number;
  title: string;
  body: string;
  state: State;
  priority: number;
  tags: string;
  assignee: string | null;
  created_by: string;
  source_session: string | null;
  created_at: number;
  updated_at: number;
  done_at: number | null;
  version: number;
}

const toItem = (row: Row): Item => ({
  rowId: row.id,
  seq: row.seq,
  ref: `B${row.seq}`,
  title: row.title,
  body: row.body,
  state: row.state,
  priority: row.priority,
  tags: row.tags ? row.tags.split(' ') : [],
  ...(row.assignee ? { assignee: row.assignee } : {}),
  createdBy: row.created_by,
  ...(row.source_session ? { sourceSession: row.source_session } : {}),
  createdAt: row.created_at,
  updatedAt: row.updated_at,
  ...(row.done_at ? { doneAt: row.done_at } : {}),
  version: row.version,
});

/** `B12`, `b12`, `#12` or `12` → 12. */
export function parseRef(ref: string): number | undefined {
  const match = /^\s*(?:[bB]|#)?(\d{1,9})\s*$/.exec(ref);
  return match ? Number(match[1]) : undefined;
}

/** Tags as stored: lower-case words of letters, digits, `.`, `_`, `-`, without a leading `#`, deduplicated. */
function cleanTags(tags: readonly string[]): string[] {
  const words = tags.map((tag) => tag.trim().replace(/^#+/, '').toLowerCase().replace(/[^\p{L}\p{N}._-]+/gu, '-').replace(/^-+|-+$/g, '').slice(0, 32)).filter(Boolean);
  return [...new Set(words)].slice(0, TAGS_MAX);
}

/**
 * Refuses text that looks like a credential (the backlog is plain text in the home database). Checked on the
 * sanitized text, the form that is stored, so an invisible character cannot split a token past the check.
 */
export function refuseSecrets(fields: Record<string, string | readonly string[] | undefined>): void {
  for (const [name, value] of Object.entries(fields)) {
    if (value === undefined) continue;
    const problem = secretProblem(sanitizeTerminalText(typeof value === 'string' ? value : value.join(' ')));
    if (problem) throw new BacklogError(`Not stored: the ${name} ${problem.replace(/^it /, '')}.`);
  }
}

export const priorityName = (priority: number): Priority => PRIORITIES[priority] ?? 'p2';

interface AddInput {
  title: string;
  body?: string;
  state?: State;
  priority?: number;
  tags?: readonly string[];
  createdBy: string;
  sourceSession?: string;
}

interface UpdateInput {
  title?: string;
  body?: string;
  state?: State;
  priority?: number;
  tags?: readonly string[];
  note?: string;
  /** Refuse when the item changed since this version was read (a board holding a stale copy). */
  version?: number;
  /** The user's own edit: may change an item another live session holds (claiming it is still refused). */
  force?: boolean;
}

interface ListQuery {
  states?: readonly State[];
  query?: string;
  limit?: number;
}

const ORDER = `CASE state WHEN 'ongoing' THEN 0 WHEN 'todo' THEN 1 WHEN 'backlog' THEN 2 ELSE 3 END, priority, updated_at DESC, seq DESC`;

export class BacklogStore {
  constructor(
    private readonly agentDb: () => AgentDb,
    readonly repo: string,
    private readonly now: () => number = Date.now,
  ) {}

  private get db() {
    return this.agentDb().db;
  }

  get(ref: string | number): Item | undefined {
    const seq = typeof ref === 'number' ? ref : parseRef(ref);
    if (seq === undefined) return undefined;
    const row = this.db.prepare('SELECT * FROM backlog WHERE repo_key = ? AND seq = ?').get(this.repo, seq) as Row | undefined;
    return row ? toItem(row) : undefined;
  }

  /** Items in `states` (every state when unset), ongoing first, then by priority and most recently updated; `total` counts past the limit. */
  list(query: ListQuery = {}): { items: Item[]; total: number } {
    const states = query.states?.length ? [...new Set(query.states)] : [...STATES];
    const where = [`repo_key = ?`, `state IN (${states.map(() => '?').join(', ')})`];
    const args: Array<string | number> = [this.repo, ...states];
    const text = query.query?.trim();
    if (text) {
      where.push(`(title LIKE ? ESCAPE '\\' OR body LIKE ? ESCAPE '\\' OR tags LIKE ? ESCAPE '\\')`);
      const like = `%${text.replace(/[\\%_]/g, (char) => `\\${char}`)}%`;
      args.push(like, like, like);
    }
    const clause = where.join(' AND ');
    const total = (this.db.prepare(`SELECT COUNT(*) AS n FROM backlog WHERE ${clause}`).get(...args) as { n: number }).n;
    const limit = query.limit === undefined ? -1 : Math.max(0, Math.floor(query.limit));
    const rows = this.db.prepare(`SELECT * FROM backlog WHERE ${clause} ORDER BY ${ORDER} LIMIT ?`).all(...args, limit) as unknown as Row[];
    return { items: rows.map(toItem), total };
  }

  counts(): Record<State, number> {
    const counts = Object.fromEntries(STATES.map((state) => [state, 0])) as Record<State, number>;
    const rows = this.db.prepare('SELECT state, COUNT(*) AS n FROM backlog WHERE repo_key = ? GROUP BY state').all(this.repo) as Array<{ state: State; n: number }>;
    for (const row of rows) counts[row.state] = row.n;
    return counts;
  }

  notes(item: Item, limit = 10): Note[] {
    const rows = this.db.prepare('SELECT at, author, text FROM backlog_notes WHERE item = ? ORDER BY at DESC, id DESC LIMIT ?').all(item.rowId, limit) as unknown as Note[];
    return rows.reverse().map((row) => ({ at: row.at, author: row.author, text: row.text }));
  }

  noteCount(item: Item): number {
    return (this.db.prepare('SELECT COUNT(*) AS n FROM backlog_notes WHERE item = ?').get(item.rowId) as { n: number }).n;
  }

  add(input: AddInput): Item {
    const title = storedText(input.title, TITLE_MAX);
    if (!title) throw new BacklogError('An item needs a title.');
    refuseSecrets({ title: input.title, body: input.body, tags: input.tags });
    const agentDb = this.agentDb();
    return agentDb.transaction(() => {
      // Numbers are never reused, even after the newest item is deleted: the high-water mark lives in `meta`.
      const key = `backlog_seq:${this.repo}`;
      const mark = Number((agentDb.db.prepare('SELECT value FROM meta WHERE key = ?').get(key) as { value: string } | undefined)?.value ?? 0);
      const max = (agentDb.db.prepare('SELECT MAX(seq) AS n FROM backlog WHERE repo_key = ?').get(this.repo) as { n: number | null }).n ?? 0;
      const seq = Math.max(mark, max) + 1;
      agentDb.db.prepare('INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value').run(key, String(seq));
      const at = this.now();
      const state = input.state ?? 'backlog';
      agentDb.db
        .prepare(
          `INSERT INTO backlog (repo_key, seq, title, body, state, priority, tags, assignee, created_by, source_session, created_at, updated_at, done_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?)`,
        )
        .run(this.repo, seq, title, storedText(input.body, BODY_MAX), state, clampPriority(input.priority), cleanTags(input.tags ?? []).join(' '), input.createdBy, input.sourceSession ?? null, at, at, state === 'done' ? at : null);
      return this.get(seq)!;
    });
  }

  /**
   * Changes an item. `ongoing` claims it for `actor` (refused while another live assignee holds it); leaving
   * `ongoing` for `backlog`/`todo` releases it; `done` stamps `done_at`. While another live session holds an item,
   * others may only add a note (the user may force an edit). A subagent may only touch items its parent holds.
   */
  update(ref: string, input: UpdateInput, actor: Actor): Item {
    refuseSecrets({ title: input.title, body: input.body, note: input.note, tags: input.tags });
    const agentDb = this.agentDb();
    return agentDb.transaction(() => {
      const item = this.get(ref);
      if (!item) throw new BacklogError(`No backlog item ${ref}.`);
      if (input.version !== undefined && input.version !== item.version) throw new BacklogError(`${item.ref} changed elsewhere — reload it.`);
      if (actor.owners && !(item.assignee && actor.owners.has(item.assignee))) {
        throw new BacklogError(`${item.ref} is not claimed by your parent: a subagent updates only the items its parent has claimed.`);
      }
      const holder = !actor.owners && item.state === 'ongoing' && item.assignee && item.assignee !== actor.id ? this.liveHolder(item.assignee) : undefined;
      const changes = (['title', 'body', 'state', 'priority', 'tags'] as const).some((field) => input[field] !== undefined);
      if (holder && (input.state === 'ongoing' || (changes && !input.force))) {
        throw new BacklogError(`${item.ref} is ongoing for ${holder}, which is still running: add a note, leave it, or ask the user.`);
      }
      const state = input.state ?? item.state;
      let assignee = item.assignee;
      if (input.state === 'ongoing' && item.assignee !== actor.id) {
        if (!actor.owners) assignee = actor.id;
      } else if (input.state === 'backlog' || input.state === 'todo') assignee = undefined;
      const note = input.note === undefined ? '' : storedText(input.note, NOTE_MAX);
      const at = this.now();
      const title = input.title === undefined ? item.title : storedText(input.title, TITLE_MAX);
      if (!title) throw new BacklogError('An item needs a title.');
      const doneAt = state === 'done' ? (item.state === 'done' ? (item.doneAt ?? at) : at) : null;
      agentDb.db
        .prepare('UPDATE backlog SET title = ?, body = ?, state = ?, priority = ?, tags = ?, assignee = ?, updated_at = ?, done_at = ?, version = version + 1 WHERE id = ?')
        .run(
          title,
          input.body === undefined ? item.body : storedText(input.body, BODY_MAX),
          state,
          input.priority === undefined ? item.priority : clampPriority(input.priority),
          input.tags === undefined ? item.tags.join(' ') : cleanTags(input.tags).join(' '),
          assignee ?? null,
          at,
          doneAt,
          item.rowId,
        );
      if (note) agentDb.db.prepare('INSERT INTO backlog_notes (item, at, author, text) VALUES (?, ?, ?, ?)').run(item.rowId, at, actor.id, note);
      return this.get(item.seq)!;
    });
  }

  /** Deletes an item and its notes. With `actor` (the agent), an item another live session holds is refused. */
  remove(ref: string, version?: number, actor?: Actor): Item {
    const agentDb = this.agentDb();
    return agentDb.transaction(() => {
      const item = this.get(ref);
      if (!item) throw new BacklogError(`No backlog item ${ref}.`);
      if (version !== undefined && version !== item.version) throw new BacklogError(`${item.ref} changed elsewhere — reload it.`);
      const holder = actor && item.state === 'ongoing' && item.assignee && item.assignee !== actor.id ? this.liveHolder(item.assignee) : undefined;
      if (holder) throw new BacklogError(`${item.ref} is ongoing for ${holder}, which is still running: leave it or ask the user.`);
      agentDb.db.prepare('DELETE FROM backlog WHERE id = ?').run(item.rowId);
      return item;
    });
  }

  /** `assignee` when it is a session whose process still runs, else undefined (a finished session's claim can be taken over). */
  liveHolder(assignee: string): string | undefined {
    const row = this.db.prepare('SELECT pid FROM sessions WHERE id = ?').get(assignee) as { pid: number | null } | undefined;
    return row && processAlive(row.pid) && row.pid !== process.pid ? assignee : undefined;
  }

  /** Sessions run by process `pid` (a subagent's parent Pi is its parent process). */
  sessionsOf(pid: number): string[] {
    return (this.db.prepare('SELECT id FROM sessions WHERE pid = ?').all(pid) as Array<{ id: string }>).map((row) => row.id);
  }
}

function clampPriority(priority: number | undefined): number {
  return priority === undefined || !Number.isInteger(priority) ? 2 : Math.min(3, Math.max(0, priority));
}
