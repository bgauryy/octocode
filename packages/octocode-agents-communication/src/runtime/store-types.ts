/**
 * The surface of the bound store that delivery and host hooks consume, and the pure helpers several store modules share.
 *
 * `Store` (store.ts) implements `StoreHost`. The capability modules (leases, documents, peers, health, …) are
 * free functions over this host, as `records.ts` is; they never import `store.ts`.
 */
import { isAbsolute, relative } from 'node:path';
import type { Database } from './database.js';
import type { CommandInputs, CommandResults } from './catalog-types.generated.js';
import type { CommandName } from './catalog.js';
import type { RecordsHost } from './records.js';

/** What delivery (`dispatch`, `host-hooks`, `proxy`) needs from the store that owns the connection. */
export interface StoreHost extends RecordsHost {
  /** Database file path (the store's `database`). */
  readonly database: string;
  /** Git common directory of this workspace's repository, else the workspace itself. */
  readonly coordinationScope: string;
  /** Fails when the database file disappeared, stopped being a regular file or was replaced. */
  checkDatabase(): void;
  /** Runs one catalog command for `session`. Async because `activity` shells out to Git; DB work inside is synchronous. */
  call<N extends CommandName>(session: string, name: N, input: CommandInputs[N]): Promise<CommandResults[N]>;
  /** Sessions of this workspace bound to a host session ID of `vendor` or attached through transport `vendor`; at most 2 rows. */
  hostIdentities(vendor: string, host: string): { id: string }[];
  /** A name free among live peers of this repository: the first of `name`, `name-2`, `name-3` … */
  uniqueName(name: string, exclude?: string): string;
  /** Bounded peer-directory text for a consumer, or '' when nothing changed since its saved view. */
  peerContext(session: string, consumer: string, generation: string): string;
  /** Whether the peer directory differs from the consumer's saved view. */
  peersChanged(session: string, consumer: string, generation: string): boolean;
}

/** A host that can also read an attachment row (completion check); `Store` provides it through `dispatch.attachment`. */
export interface AttachmentHost extends StoreHost {
  attachment(session: string): CommandResults['attach'];
}

export type { Database };

/** A record whose own keys must keep their nulls (an envelope with path, from, type, timestamp and data). */
function isEnvelope(value: object): boolean {
  return ['path', 'from', 'type', 'timestamp', 'data'].every(key => key in value);
}

/** Removes null (and undefined) object members everywhere, in place, except inside record envelopes and message parts. Returns `value`. */
export function stripNulls<T>(value: T): T {
  strip(value);
  return value;
}

function strip(value: unknown): void {
  if (Array.isArray(value)) {
    for (const item of value) strip(item);
  } else if (typeof value === 'object' && value !== null) {
    if (isEnvelope(value)) return;
    for (const key of Object.keys(value)) {
      const item: unknown = Reflect.get(value, key);
      if (item === null || item === undefined) Reflect.deleteProperty(value, key);
      else if (key !== 'parts') strip(item);
    }
  }
}

/** `path` relative to `base` as a slash-separated string; `.` for the base itself; null when `path` is not below `base`. */
function below(path: string, base: string): string | null {
  const value = relative(base, path);
  if (value === '') return '.';
  return value === '..' || value.startsWith('../') || value.startsWith('..\\') || isAbsolute(value) ? null : value.replaceAll('\\', '/');
}

/** Whether `path` is `base` or lies below it (lexically). */
export function isWithin(path: string, base: string): boolean {
  return below(path, base) !== null;
}

/** `path` relative to `base` with `/` separators, or `path` unchanged when it lies elsewhere. */
export function relativePath(path: string, base: string): string {
  return below(path, base) ?? path;
}
