import type { AgentDb } from '../agentdb/db.js';
import { sanitizeTerminalText, secretProblem, storedText } from '../shared/sanitize.js';
import { clipText } from '../shared/util.js';
import { ftsQuery, jaccard, memoryLabel, normalizeTitle, stem, terms } from './query.js';

export const MEMORY_KINDS = ['preference', 'fact', 'decision', 'gotcha', 'procedure'] as const;
export type MemoryKind = (typeof MEMORY_KINDS)[number];
type MemoryScope = 'global' | 'project';
/** Which memories a read sees: `all` is global plus this repository's project memories. */
export type ScopeFilter = MemoryScope | 'all';

export const TITLE_MAX = 120;
export const BODY_MAX = 1500;
const KEYWORDS_MAX = 200;

export interface Memory {
  id: number;
  scope: MemoryScope;
  repoKey: string | null;
  kind: MemoryKind;
  title: string;
  body: string;
  keywords: string;
  pinned: boolean;
  author: string;
  sourceSession: string | null;
  createdAt: number;
  updatedAt: number;
  lastUsed: number | null;
  useCount: number;
}

/** A search hit: `score` is higher-is-better (negated BM25, or the LIKE fallback's weighted term count). */
interface MemoryHit extends Memory {
  score: number;
}

export interface MemoryInput {
  scope?: MemoryScope;
  kind?: MemoryKind;
  title?: string;
  body?: string;
  keywords?: string;
  pinned?: boolean;
}

/** A refused write: a secret, a near-duplicate, or a missing/unknown memory. */
export class MemoryError extends Error {}

/** Refuses title, body or keywords that look like a secret. Checked on the sanitized text, the form that is stored: an invisible character must not split a token past it. */
export function refuseMemorySecrets(input: Pick<MemoryInput, 'title' | 'body' | 'keywords'>): void {
  for (const text of [input.title, input.body, input.keywords]) {
    const problem = text ? secretProblem(sanitizeTerminalText(text)) : undefined;
    if (problem) throw new MemoryError(`Refused: this looks like a secret (${problem}). Never store credentials in memory.`);
  }
}

/** BM25 column weights for (title, keywords, body), in memories_fts column order. */
const BM25_WEIGHTS = '5.0, 3.0, 1.0';
/** A project hit ranks this much above an equally good global one. */
const PROJECT_BOOST = 1.15;
/** Token overlap (title + body) at which a new memory counts as a duplicate of an existing one. */
const DUPLICATE_JACCARD = 0.6;
/** Rows the LIKE fallback scores in memory (newest first); far more than a person keeps. */
const LIKE_SCAN_LIMIT = 2000;
const SETTING_AUTO = 'memory_auto';

type Row = Record<string, unknown>;

function toMemory(row: Row): Memory {
  return {
    id: Number(row['id']),
    scope: row['scope'] === 'global' ? 'global' : 'project',
    repoKey: (row['repo_key'] as string | null) ?? null,
    kind: (MEMORY_KINDS as readonly string[]).includes(String(row['kind'])) ? (row['kind'] as MemoryKind) : 'fact',
    title: String(row['title'] ?? ''),
    body: String(row['body'] ?? ''),
    keywords: String(row['keywords'] ?? ''),
    pinned: Number(row['pinned']) === 1,
    author: String(row['author'] ?? ''),
    sourceSession: (row['source_session'] as string | null) ?? null,
    createdAt: Number(row['created_at']),
    updatedAt: Number(row['updated_at']),
    lastUsed: row['last_used'] == null ? null : Number(row['last_used']),
    useCount: Number(row['use_count'] ?? 0),
  };
}

/**
 * Memories of one repository (`repoKey`) plus the global ones, in the shared agent DB. Every method is one statement or
 * one transaction; the DB's WAL mode lets sessions read and write side by side.
 */
export class MemoryStore {
  constructor(
    private readonly agentDb: AgentDb,
    readonly repoKey: string,
  ) {}

  private get db() {
    return this.agentDb.db;
  }

  /** SQL and parameters restricting `m` to the memories `scope` sees from this repository. */
  private scopeSql(scope: ScopeFilter): { sql: string; params: string[] } {
    if (scope === 'global') return { sql: "m.scope = 'global'", params: [] };
    if (scope === 'project') return { sql: "m.scope = 'project' AND m.repo_key = ?", params: [this.repoKey] };
    return { sql: "(m.scope = 'global' OR (m.scope = 'project' AND m.repo_key = ?))", params: [this.repoKey] };
  }

  /** A memory visible from this repository (global, or project of this repo), else undefined. */
  get(id: number): Memory | undefined {
    const scope = this.scopeSql('all');
    const row = this.db.prepare(`SELECT m.* FROM memories m WHERE m.id = ? AND ${scope.sql}`).get(id, ...scope.params) as Row | undefined;
    return row ? toMemory(row) : undefined;
  }

  /** Pinned first, then most recently updated. */
  list(scope: ScopeFilter = 'all', limit = 50, options: { pinnedOnly?: boolean } = {}): Memory[] {
    const where = this.scopeSql(scope);
    const pinned = options.pinnedOnly ? ' AND m.pinned = 1' : '';
    const rows = this.db.prepare(`SELECT m.* FROM memories m WHERE ${where.sql}${pinned} ORDER BY m.pinned DESC, m.updated_at DESC, m.id DESC LIMIT ?`).all(...where.params, limit) as Row[];
    return rows.map(toMemory);
  }

  count(scope: ScopeFilter = 'all'): number {
    const where = this.scopeSql(scope);
    const row = this.db.prepare(`SELECT count(*) AS n FROM memories m WHERE ${where.sql}`).get(...where.params) as Row;
    return Number(row['n']);
  }

  /**
   * Best matches for the words of `query`, best first. With FTS5: BM25 over title, keywords and body (weighted 5/3/1).
   * Without it: the same column weights times each term's inverse document frequency. Project hits get a small boost either way.
   */
  search(query: string, scope: ScopeFilter = 'all', limit = 10, exclude: ReadonlySet<number> = new Set()): MemoryHit[] {
    const match = ftsQuery(query);
    if (!match) return [];
    const hits = this.agentDb.fts ? this.searchFts(match, scope, limit * 3, exclude) : this.searchLike([...new Set(terms(query).map(stem))], scope);
    return hits
      .filter((hit) => !exclude.has(hit.id))
      .map((hit) => (hit.scope === 'project' ? { ...hit, score: hit.score * PROJECT_BOOST } : hit))
      .sort((a, b) => b.score - a.score || b.updatedAt - a.updatedAt)
      .slice(0, limit);
  }

  private searchFts(match: string, scope: ScopeFilter, limit: number, exclude: ReadonlySet<number>): MemoryHit[] {
    const where = this.scopeSql(scope);
    const rows = this.db
      .prepare(
        `SELECT m.*, bm25(memories_fts, ${BM25_WEIGHTS}) AS bm25_score FROM memories_fts JOIN memories m ON m.id = memories_fts.rowid ` +
          `WHERE memories_fts MATCH ? AND ${where.sql} AND m.id NOT IN (SELECT value FROM json_each(?)) ORDER BY bm25_score LIMIT ?`,
      )
      .all(match, ...where.params, JSON.stringify([...exclude]), limit) as Row[];
    // bm25() is lower-is-better and negative for matches; negate so every score reads higher-is-better.
    return rows.map((row) => ({ ...toMemory(row), score: -Number(row['bm25_score']) }));
  }

  /** BM25-like without FTS5: per-column weights (5/3/1) times each term's inverse document frequency; `words` are stems. */
  private searchLike(words: string[], scope: ScopeFilter): MemoryHit[] {
    const where = this.scopeSql(scope);
    const rows = this.db.prepare(`SELECT m.* FROM memories m WHERE ${where.sql} ORDER BY m.updated_at DESC LIMIT ?`).all(...where.params, LIKE_SCAN_LIMIT) as Row[];
    const docs = rows.map((row) => {
      const memory = toMemory(row);
      const stems = (text: string) => new Set(terms(text).map(stem));
      return { memory, title: stems(memory.title), keywords: stems(memory.keywords), body: stems(memory.body) };
    });
    const idf = new Map(
      words.map((word) => {
        const df = docs.filter((doc) => doc.title.has(word) || doc.keywords.has(word) || doc.body.has(word)).length;
        return [word, Math.log(1 + (docs.length - df + 0.5) / (df + 0.5))];
      }),
    );
    return docs.flatMap((doc) => {
      const score = words.reduce((sum, word) => sum + idf.get(word)! * ((doc.title.has(word) ? 5 : 0) + (doc.keywords.has(word) ? 3 : 0) + (doc.body.has(word) ? 1 : 0)), 0);
      return score > 0 ? [{ ...doc.memory, score }] : [];
    });
  }

  /**
   * Adds a memory, or updates memory `id`. Refuses text that looks like a secret, and a new memory that repeats one in
   * the same scope (same title, or most of the same words), naming it so the caller can update it instead.
   */
  set(input: MemoryInput & { id?: number; author: string; sourceSession?: string; hideProject?: boolean }, now = Date.now()): { memory: Memory; created: boolean } {
    refuseMemorySecrets(input);
    return this.agentDb.transaction(() => (input.id === undefined ? this.insert(input, now) : this.update(input.id, input, now)));
  }

  private insert(input: MemoryInput & { author: string; sourceSession?: string; hideProject?: boolean }, now: number): { memory: Memory; created: boolean } {
    const title = storedText(input.title, TITLE_MAX);
    if (!title) throw new MemoryError('A new memory needs a title.');
    const body = storedText(input.body, BODY_MAX);
    const scope = input.scope ?? 'project';
    const duplicate = this.duplicateOf(scope, title, body);
    // An untrusted project cannot read project memories: name the duplicate without showing it.
    if (duplicate && input.hideProject && duplicate.scope === 'project') {
      throw new MemoryError(`A similar project memory (${memoryLabel(duplicate.id)}) exists, hidden in this untrusted project. Change the title, or ask the user to /trust the project.`);
    }
    if (duplicate) throw new MemoryError(`Similar memory ${memoryLabel(duplicate.id)} exists: "${duplicate.title}". Call set with id:"${memoryLabel(duplicate.id)}" to update it, or change the title.`);
    const result = this.db
      .prepare(
        'INSERT INTO memories (scope, repo_key, kind, title, body, keywords, pinned, author, source_session, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)',
      )
      .run(scope, scope === 'project' ? this.repoKey : null, input.kind ?? 'fact', title, body, storedText(input.keywords, KEYWORDS_MAX), input.pinned ? 1 : 0, input.author, input.sourceSession ?? null, now, now);
    return { memory: this.get(Number(result.lastInsertRowid))!, created: true };
  }

  private update(id: number, input: MemoryInput, now: number): { memory: Memory; created: boolean } {
    const current = this.get(id);
    if (!current) throw new MemoryError(`No memory ${memoryLabel(id)} here (it may belong to another repository).`);
    const title = input.title === undefined ? current.title : storedText(input.title, TITLE_MAX);
    if (!title) throw new MemoryError('A memory needs a title.');
    const scope = input.scope ?? current.scope;
    this.db
      .prepare('UPDATE memories SET scope = ?, repo_key = ?, kind = ?, title = ?, body = ?, keywords = ?, pinned = ?, updated_at = ? WHERE id = ?')
      .run(
        scope,
        scope === 'project' ? (current.scope === 'project' ? current.repoKey : this.repoKey) : null,
        input.kind ?? current.kind,
        title,
        input.body === undefined ? current.body : storedText(input.body, BODY_MAX),
        input.keywords === undefined ? current.keywords : storedText(input.keywords, KEYWORDS_MAX),
        (input.pinned ?? current.pinned) ? 1 : 0,
        now,
        id,
      );
    return { memory: this.get(id)!, created: false };
  }

  /** An existing memory in `scope` with the same normalized title, or whose title+body words mostly match. */
  private duplicateOf(scope: MemoryScope, title: string, body: string): Memory | undefined {
    const normalized = normalizeTitle(title);
    const words = terms(`${title} ${body}`);
    const candidates = [...this.search(`${title} ${body}`, scope, 5), ...this.list(scope, 200)];
    return candidates.find((memory) => (normalized !== '' && normalizeTitle(memory.title) === normalized) || jaccard(words, terms(`${memory.title} ${memory.body}`)) >= DUPLICATE_JACCARD);
  }

  delete(id: number): Memory {
    const current = this.get(id);
    if (!current) throw new MemoryError(`No memory ${memoryLabel(id)} here (it may belong to another repository).`);
    this.db.prepare('DELETE FROM memories WHERE id = ?').run(id);
    return current;
  }

  /** Records that `ids` were handed to the model. */
  markUsed(ids: readonly number[], now = Date.now()): void {
    if (ids.length === 0) return;
    this.agentDb.transaction(() => {
      const statement = this.db.prepare('UPDATE memories SET last_used = ?, use_count = use_count + 1 WHERE id = ?');
      for (const id of ids) statement.run(now, id);
    });
  }

  /** `/octocode memory auto on|off`, shared by every session (the environment can still turn it off). */
  autoSetting(): boolean {
    const row = this.db.prepare('SELECT value FROM meta WHERE key = ?').get(SETTING_AUTO) as Row | undefined;
    return row?.['value'] !== 'off';
  }

  setAutoSetting(on: boolean): void {
    this.db.prepare('INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value').run(SETTING_AUTO, on ? 'on' : 'off');
  }
}

/** `M3 (project, decision, pinned) Title — body`, one line, body clipped to `bodyChars`. */
export function memoryLine(memory: Memory, bodyChars = 160): string {
  const tags = [memory.scope, memory.kind, ...(memory.pinned ? ['pinned'] : [])].join(', ');
  const body = memory.body.replace(/\s+/g, ' ').trim();
  const clipped = clipText(body, bodyChars);
  return `${memoryLabel(memory.id)} (${tags}) ${memory.title.replace(/\s+/g, ' ')}${clipped ? ` — ${clipped}` : ''}`;
}
