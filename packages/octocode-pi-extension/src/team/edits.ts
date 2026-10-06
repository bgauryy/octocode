import path from 'node:path';
import type { DatabaseSync } from 'node:sqlite';

/** Who changed which file, kept a day in the team database, for `coordinate changes`. */

/** The latest change to one file by an agent, and how many changes it had in the window. */
export interface Change {
  /** Relative to the workspace, `/`-separated. */
  path: string;
  agent: string;
  tool: string;
  at: number;
  count: number;
}

const EDIT_RETENTION_MS = 24 * 3_600_000;

type Row = Record<string, unknown>;
const num = (value: unknown) => (typeof value === 'number' ? value : 0);

export class EditLog {
  constructor(
    private readonly db: DatabaseSync,
    readonly workspace: string,
  ) {}

  /** Records that `agent` changed `files` (absolute) with `tool`; files outside the workspace are skipped. Drops rows past retention. */
  record(agent: string, tool: string, files: string[], now = Date.now()): void {
    this.prune(now);
    const insert = this.db.prepare('INSERT INTO edits (workspace, path, agent, tool, at) VALUES (?, ?, ?, ?, ?)');
    for (const file of files) {
      const relative = path.relative(this.workspace, path.resolve(this.workspace, file));
      if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) continue;
      insert.run(this.workspace, relative.split(path.sep).join('/'), agent, tool, now);
    }
  }

  /**
   * Files agents changed in the last `withinMs`, newest first, one row per file (its latest change and the count),
   * optionally only under `prefixes` (workspace-relative, `/`-separated).
   */
  changes(options: { withinMs: number; prefixes?: string[] }, now = Date.now()): Change[] {
    // SQLite takes the bare columns from the row holding MAX(at): the latest agent and tool per file.
    const rows = this.db
      .prepare('SELECT path, agent, tool, MAX(at) AS at, COUNT(*) AS count FROM edits WHERE workspace = ? AND at > ? GROUP BY path ORDER BY at DESC, path')
      .all(this.workspace, now - options.withinMs) as Row[];
    const prefixes = options.prefixes?.map((prefix) => prefix.replace(/\/+$/, ''));
    const under = (file: string) => !prefixes?.length || prefixes.some((prefix) => prefix === '' || file === prefix || file.startsWith(`${prefix}/`));
    return rows.map((row) => ({ path: String(row['path']), agent: String(row['agent']), tool: String(row['tool']), at: num(row['at']), count: num(row['count']) })).filter((change) => under(change.path));
  }

  prune(now = Date.now()): void {
    this.db.prepare('DELETE FROM edits WHERE at < ?').run(now - EDIT_RETENTION_MS);
  }
}
