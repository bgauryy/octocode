import type { AgentDb } from '../agentdb/db.js';
import { USER_AUTHOR } from '../agentdb/author.js';
import { sessionRunningPid } from '../agentdb/liveness.js';
import { readMeta, writeMeta } from '../agentdb/meta.js';
import { storedText, visibleControls } from '../shared/sanitize.js';
import { BacklogError, changeText, clampPriority, cleanList, cleanTags, parseRef, refuseSecrets, storedTitle, STATES, type Actor, type Item, type Note, type NoteKind, type State } from './item.js';
import { refText, toItem, type Row } from './row.js';

/**
 * The per-repository backlog in the shared agent database (`backlog`, `backlog_notes`; schema in src/agentdb). Items are
 * numbered per repository (`B12`); each call is one transaction, so several Pi processes can work the same board.
 *
 *   backlog   triage inbox: everything an agent proposes lands here
 *   todo      accepted, not started
 *   ongoing   claimed by one session or subagent (the assignee)
 *   done      finished, with a note saying what changed and how it was verified
 *
 * An item has a title, a goal (what done means), where it applies (`paths`), why it matters, the evidence for it and a
 * plan, and a body for whatever else. The store keeps when it was created,
 * last claimed (`started_at`), updated and done. An item may name the item it came from (`from`: a split, follow-up or finding), so the origin lists what came of it.
 * Every update writes a `change` note with what it changed (a replaced text is kept in full), next to the agent's own
 * notes, which say why.
 */

/** Items with the number of the item each came from; filters and order use the `b.` alias. */
const SELECT = 'SELECT b.*, f.seq AS from_seq FROM backlog b LEFT JOIN backlog f ON f.id = b.from_item';

interface AddInput {
  title: string;
  goal?: string;
  body?: string;
  paths?: readonly string[];
  why?: string;
  evidence?: readonly string[];
  plan?: string;
  state?: State;
  priority?: number;
  tags?: readonly string[];
  /** The item this one came from (`B12`). */
  from?: string;
  createdBy: string;
  sourceSession?: string;
}

export interface UpdateInput {
  title?: string;
  goal?: string;
  body?: string;
  paths?: readonly string[];
  why?: string;
  evidence?: readonly string[];
  plan?: string;
  state?: State;
  priority?: number;
  tags?: readonly string[];
  /** The item this one came from (`B12`); null clears the link. */
  from?: string | null;
  note?: string;
  /** Refuse when the item changed since this version was read (a board holding a stale copy). */
  version?: number;
  /** The user's own edit: may change an item another live session holds (claiming it is still refused). */
  force?: boolean;
}

interface ListQuery {
  states?: readonly State[];
  query?: string;
  /** Only the items that came from this one (`B12`). */
  from?: string;
  limit?: number;
  offset?: number;
}

const ORDER = `CASE b.state WHEN 'ongoing' THEN 0 WHEN 'todo' THEN 1 WHEN 'backlog' THEN 2 ELSE 3 END, b.priority, b.updated_at DESC, b.seq DESC`;

const SEARCHED = ['title', 'goal', 'body', 'paths', 'why', 'evidence', 'plan'] as const;

const likeEscape = (text: string) => text.replace(/[\\%_]/g, (char) => `\\${char}`);

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
    const row = this.db.prepare(`${SELECT} WHERE b.repo_key = ? AND b.seq = ?`).get(this.repo, seq) as Row<State> | undefined;
    return row ? toItem(row) : undefined;
  }

  /** The items that came from `item`, in board order. */
  derived(item: Item): Item[] {
    const rows = this.db.prepare(`${SELECT} WHERE b.from_item = ? ORDER BY ${ORDER}`).all(item.rowId) as unknown as Row<State>[];
    return rows.map(toItem);
  }

  /** The row id of the item `ref` names, refused when it is missing, is `item` itself, or already comes from `item`. */
  private origin(ref: string, item?: Item): number {
    const target = this.get(ref);
    if (!target) throw new BacklogError(`No backlog item ${visibleControls(ref)} to link from.`);
    if (!item) return target.rowId;
    if (target.rowId === item.rowId) throw new BacklogError(`${item.ref} cannot come from itself.`);
    const seen = new Set<number>();
    for (let at: Item | undefined = target; at?.from !== undefined && !seen.has(at.seq); at = this.get(at.from)) {
      seen.add(at.seq);
      if (at.from === item.seq) throw new BacklogError(`${target.ref} already comes from ${item.ref}: the link would make a loop.`);
    }
    return target.rowId;
  }

  /** Items in `states` (every state when unset), ongoing first, then by priority and most recently updated; `total` counts past the limit. */
  list(query: ListQuery = {}): { items: Item[]; total: number } {
    const states = query.states?.length ? [...new Set(query.states)] : [...STATES];
    const where = [`b.repo_key = ?`, `b.state IN (${states.map(() => '?').join(', ')})`];
    const args: Array<string | number> = [this.repo, ...states];
    const text = query.query?.trim();
    if (text) {
      // Every text field matches any substring (so `backlog/store` finds a path); a tag matches only whole (`ui` is not found in `build`), with or without `#`.
      where.push(`(${SEARCHED.map((column) => `b.${column} LIKE ? ESCAPE '\\'`).join(' OR ')} OR (' ' || b.tags || ' ') LIKE ? ESCAPE '\\')`);
      const like = `%${likeEscape(text)}%`;
      args.push(...SEARCHED.map(() => like), `% ${likeEscape(text.replace(/^#+/, ''))} %`);
    }
    if (query.from !== undefined) {
      where.push('b.from_item = ?');
      args.push(this.origin(query.from));
    }
    const clause = where.join(' AND ');
    const total = (this.db.prepare(`SELECT COUNT(*) AS n FROM backlog b WHERE ${clause}`).get(...args) as { n: number }).n;
    const limit = query.limit === undefined ? -1 : Math.max(0, Math.floor(query.limit));
    const rows = this.db.prepare(`${SELECT} WHERE ${clause} ORDER BY ${ORDER} LIMIT ? OFFSET ?`).all(...args, limit, query.offset ?? 0) as unknown as Row<State>[];
    return { items: rows.map(toItem), total };
  }

  counts(): Record<State, number> {
    const counts = Object.fromEntries(STATES.map((state) => [state, 0])) as Record<State, number>;
    const rows = this.db.prepare('SELECT state, COUNT(*) AS n FROM backlog WHERE repo_key = ? GROUP BY state').all(this.repo) as Array<{ state: State; n: number }>;
    for (const row of rows) counts[row.state] = row.n;
    return counts;
  }

  /** The newest `limit` notes after skipping `offset` of them, oldest first; a negative `limit` is every note. */
  notes(item: Item, limit = 10, offset = 0): Note[] {
    const rows = this.db.prepare('SELECT at, author, kind, text FROM backlog_notes WHERE item = ? ORDER BY at DESC, id DESC LIMIT ? OFFSET ?').all(item.rowId, limit, offset) as unknown as Note[];
    return rows.reverse().map((row) => ({ at: row.at, author: row.author, kind: row.kind, text: row.text }));
  }

  noteCount(item: Item): number {
    return (this.db.prepare('SELECT COUNT(*) AS n FROM backlog_notes WHERE item = ?').get(item.rowId) as { n: number }).n;
  }

  add(input: AddInput): Item {
    const title = storedTitle(input.title);
    if (!title) throw new BacklogError('An item needs a title.');
    // A person may jot a bare title; whatever an agent adds says how to tell it is done.
    if (input.createdBy !== USER_AUTHOR && !storedText(input.goal)) throw new BacklogError('add needs a goal: the done criterion, a check someone else can run.');
    refuseSecrets({ title: input.title, goal: input.goal, body: input.body, paths: input.paths, why: input.why, evidence: input.evidence, plan: input.plan, tags: input.tags });
    const agentDb = this.agentDb();
    return agentDb.transaction(() => {
      // Numbers are never reused, even after the newest item is deleted: the high-water mark lives in `meta`.
      const key = `backlog_seq:${this.repo}`;
      const mark = Number(readMeta(agentDb.db, key) ?? 0);
      const max = (agentDb.db.prepare('SELECT MAX(seq) AS n FROM backlog WHERE repo_key = ?').get(this.repo) as { n: number | null }).n ?? 0;
      const seq = Math.max(mark, max) + 1;
      writeMeta(agentDb.db, key, String(seq));
      const from = input.from === undefined ? null : this.origin(input.from);
      const at = this.now();
      const state = input.state ?? 'backlog';
      agentDb.db
        .prepare(
          `INSERT INTO backlog (repo_key, seq, title, goal, body, paths, why, evidence, plan, state, priority, tags, assignee, created_by, source_session, created_at, updated_at, started_at, done_at, from_item)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?)`,
        )
        .run(
          this.repo,
          seq,
          title,
          storedText(input.goal),
          storedText(input.body),
          cleanList(input.paths ?? []).join('\n'),
          storedText(input.why),
          cleanList(input.evidence ?? []).join('\n'),
          storedText(input.plan),
          state,
          clampPriority(input.priority),
          cleanTags(input.tags ?? []).join(' '),
          input.createdBy,
          input.sourceSession ?? null,
          at,
          at,
          state === 'ongoing' ? at : null,
          state === 'done' ? at : null,
          from,
        );
      const item = this.get(seq)!;
      if (item.from !== undefined) this.log(item, at, input.createdBy, 'change', `created from ${refText(item.from)}`);
      return item;
    });
  }

  /**
   * Changes an item. `ongoing` claims it for `actor` (refused while another live assignee holds it); leaving
   * `ongoing` for `backlog`/`todo` releases it; `done` stamps `done_at`. While another live session holds an item,
   * others may only add a note (the user may force an edit). A subagent may only touch items its parent holds.
   */
  update(ref: string, input: UpdateInput, actor: Actor): Item {
    refuseSecrets({ title: input.title, goal: input.goal, body: input.body, paths: input.paths, why: input.why, evidence: input.evidence, plan: input.plan, note: input.note, tags: input.tags });
    const agentDb = this.agentDb();
    return agentDb.transaction(() => {
      const item = this.get(ref);
      if (!item) throw new BacklogError(`No backlog item ${ref}.`);
      if (input.version !== undefined && input.version !== item.version) throw new BacklogError(`${item.ref} changed elsewhere — reload it.`);
      if (actor.owners && !(item.assignee && actor.owners.has(item.assignee))) {
        throw new BacklogError(`${item.ref} is not claimed by your parent: a subagent updates only the items its parent has claimed.`);
      }
      const holder = !actor.owners && item.state === 'ongoing' && item.assignee && item.assignee !== actor.id ? this.liveHolder(item.assignee) : undefined;
      const changes = (['title', 'goal', 'body', 'paths', 'why', 'evidence', 'plan', 'state', 'priority', 'tags', 'from'] as const).some((field) => input[field] !== undefined);
      if (holder && (input.state === 'ongoing' || (changes && !input.force))) {
        throw new BacklogError(`${item.ref} is ongoing for ${holder}, which is still running: add a note, leave it, or ask the user.`);
      }
      const state = input.state ?? item.state;
      let assignee = item.assignee;
      if (input.state === 'ongoing' && item.assignee !== actor.id) {
        if (!actor.owners) assignee = actor.id;
      } else if (input.state === 'backlog' || input.state === 'todo') assignee = undefined;
      const note = input.note === undefined ? '' : storedText(input.note);
      const at = this.now();
      const title = input.title === undefined ? item.title : storedTitle(input.title);
      if (!title) throw new BacklogError('An item needs a title.');
      const doneAt = state === 'done' ? (item.state === 'done' ? (item.doneAt ?? at) : at) : null;
      // A claim (or a takeover) starts the clock; releasing the item clears it; done keeps it, so the cycle time stays.
      const claimed = state === 'ongoing' && (item.state !== 'ongoing' || assignee !== item.assignee);
      const startedAt = claimed ? at : state === 'backlog' || state === 'todo' ? null : (item.startedAt ?? null);
      const fromItem = input.from === undefined ? (item.from === undefined ? null : this.get(item.from)!.rowId) : input.from === null ? null : this.origin(input.from, item);
      agentDb.db
        .prepare('UPDATE backlog SET title = ?, goal = ?, body = ?, paths = ?, why = ?, evidence = ?, plan = ?, state = ?, priority = ?, tags = ?, assignee = ?, updated_at = ?, started_at = ?, done_at = ?, from_item = ?, version = version + 1 WHERE id = ?')
        .run(
          title,
          input.goal === undefined ? item.goal : storedText(input.goal),
          input.body === undefined ? item.body : storedText(input.body),
          (input.paths === undefined ? item.paths : cleanList(input.paths)).join('\n'),
          input.why === undefined ? item.why : storedText(input.why),
          (input.evidence === undefined ? item.evidence : cleanList(input.evidence)).join('\n'),
          input.plan === undefined ? item.plan : storedText(input.plan),
          state,
          input.priority === undefined ? item.priority : clampPriority(input.priority),
          input.tags === undefined ? item.tags.join(' ') : cleanTags(input.tags).join(' '),
          assignee ?? null,
          at,
          startedAt,
          doneAt,
          fromItem,
          item.rowId,
        );
      const updated = this.get(item.seq)!;
      // What changed first, then why: notes at one time read in insertion order.
      const change = changeText(item, updated);
      if (change) this.log(item, at, actor.id, 'change', change);
      if (note) this.log(item, at, actor.id, 'note', note);
      return updated;
    });
  }

  private log(item: Item, at: number, author: string, kind: NoteKind, text: string): void {
    this.db.prepare('INSERT INTO backlog_notes (item, at, author, kind, text) VALUES (?, ?, ?, ?, ?)').run(item.rowId, at, author, kind, text);
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
      // The items that came from it keep a trace of the link in their change log.
      const at = this.now();
      for (const child of this.derived(item)) {
        agentDb.db.prepare('UPDATE backlog SET from_item = NULL, updated_at = ?, version = version + 1 WHERE id = ?').run(at, child.rowId);
        this.log(child, at, actor?.id ?? USER_AUTHOR, 'change', `from ${item.ref} → none (${item.ref} "${item.title}" was removed)`);
      }
      agentDb.db.prepare('DELETE FROM backlog WHERE id = ?').run(item.rowId);
      return item;
    });
  }

  /** `assignee` when it is a session whose process still runs, else undefined (a finished session's claim can be taken over). */
  liveHolder(assignee: string): string | undefined {
    const pid = sessionRunningPid(this.db, assignee);
    return pid !== undefined && pid !== process.pid ? assignee : undefined;
  }
}
