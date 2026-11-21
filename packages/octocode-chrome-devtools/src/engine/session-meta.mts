// Per-port session metadata kept next to the session file: the origin allowlist and tab groups.
// Reads files only unless updateSessionMeta is called, so the sandboxed runner can import it.
import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'fs';
import { join } from 'path';
import { stateDirFor } from './session-registry.mjs';

/** What `open` records for one browser port. */
export type SessionMeta = {
  /** Origins plans and new tabs may navigate to; unset means unrestricted. */
  allowOrigins?: string[];
  /** Tab groups made with the `group` command: name to target ids, in open order. */
  groups?: Record<string, string[]>;
  /** Browser context of each private group, by group name; `group close` and `cleanup` dispose them. */
  contexts?: Record<string, string>;
  /** Validated `open --emulation` object the runner re-applies on every connection to this port. */
  emulation?: Record<string, unknown>;
};

export const metaFileIn = (stateDir: string, port: string | number) =>
  join(stateDir, `cdp-session-meta-${port}.json`);

const isStringList = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every(item => typeof item === 'string');

/** The plain object stored under `key`, if any. */
const recordAt = (raw: object, key: string): object | undefined => {
  const value = (raw as Record<string, unknown>)[key];
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? value
    : undefined;
};

/** Keeps only well-formed fields: a damaged or foreign file reads as empty metadata. */
function sanitize(raw: unknown): SessionMeta {
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) return {};
  const meta: SessionMeta = {};
  if ('allowOrigins' in raw && isStringList(raw.allowOrigins))
    meta.allowOrigins = raw.allowOrigins;
  const contexts = recordAt(raw, 'contexts');
  if (contexts)
    meta.contexts = Object.fromEntries(
      Object.entries(contexts).filter(([, id]) => typeof id === 'string')
    );
  const emulation = recordAt(raw, 'emulation');
  if (emulation) meta.emulation = emulation as Record<string, unknown>;
  const groups = recordAt(raw, 'groups');
  if (groups)
    meta.groups = Object.fromEntries(
      Object.entries(groups).filter(([, ids]) => isStringList(ids))
    );
  return meta;
}

export function readSessionMeta(
  port: string | number,
  workspace = process.cwd()
): SessionMeta {
  try {
    return sanitize(
      JSON.parse(readFileSync(metaFileIn(stateDirFor(workspace), port), 'utf8'))
    );
  } catch {
    return {};
  }
}

/**
 * Shallow-merges `patch` into the port's metadata and writes it atomically (mode 0600).
 * A key set to `undefined` is removed. Returns the stored metadata.
 */
export function updateSessionMeta(
  port: string | number,
  patch: { [K in keyof SessionMeta]?: SessionMeta[K] | undefined },
  workspace = process.cwd()
): SessionMeta {
  const dir = stateDirFor(workspace);
  const merged: Record<string, unknown> = {
    ...readSessionMeta(port, workspace),
    ...patch,
  };
  for (const key of Object.keys(merged))
    if (merged[key] === undefined) delete merged[key];
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  const file = metaFileIn(dir, port);
  const temp = `${file}.${process.pid}.${Math.random().toString(36).slice(2)}.tmp`;
  writeFileSync(temp, JSON.stringify(merged), { mode: 0o600, flag: 'wx' });
  renameSync(temp, file);
  return merged;
}

export function removeSessionMeta(
  port: string | number,
  workspace = process.cwd()
) {
  rmSync(metaFileIn(stateDirFor(workspace), port), { force: true });
}

/** `https://a.example/x?y` as its origin; throws on text that is not an absolute http(s) origin. */
export function normalizeOrigin(text: string): string {
  let url: URL;
  try {
    url = new URL(text.trim());
  } catch {
    throw new Error(
      `Invalid origin ${JSON.stringify(text)}: use an origin like https://app.example.com`
    );
  }
  if (url.origin === 'null')
    throw new Error(
      `Invalid origin ${JSON.stringify(text)}: use an http(s) origin like https://app.example.com`
    );
  return url.origin;
}

/** The origin a navigation to `url` reaches, or `url` itself when it has none (data:, chrome-error:). */
function originOf(url: string): string {
  try {
    const { origin } = new URL(url);
    return origin === 'null' ? url.split(/[?#]/)[0] : origin;
  } catch {
    return url;
  }
}

/** The error text for a url outside the list, or null when allowed. about:blank is always allowed. */
export function originViolation(
  url: string,
  allowOrigins: readonly string[] | undefined
): string | null {
  if (!allowOrigins || url === 'about:blank') return null;
  const origin = originOf(url);
  if (allowOrigins.includes(origin)) return null;
  return `Origin ${origin} is not allowed for this session; allowed origins: ${allowOrigins.join(', ') || '(none)'}. Change them with open --port <n> --allow-origin <csv>.`;
}

/** A url as origin plus path: query strings and fragments can carry tokens. */
export function originAndPath(url: string): string {
  try {
    const parsed = new URL(url);
    return parsed.origin === 'null'
      ? url.split(/[?#]/)[0]
      : `${parsed.origin}${parsed.pathname}`;
  } catch {
    return url.split(/[?#]/)[0];
  }
}
