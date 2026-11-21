// Shared by the evidence recipes (page-html, page-archive, page-resources, console-log, downloads):
// stdout protocol lines, name sanitizing, file writes with a digest, and a bounded page load.
import { createHash } from 'node:crypto';
import { chmodSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import type { Payload, Session } from '../cdp-connection.mjs';

export const MAX_STDOUT_ITEMS = Math.max(
  1,
  Number.parseInt(process.env.MAX_STDOUT_ITEMS ?? '10', 10) || 10
);

export const out = (line: string) => {
  process.stdout.write(`${line}\n`);
};

/** One saved file: its path relative to the run folder, byte size and sha256. */
export type SavedFile = { file: string; bytes: number; sha256: string };

/** Writes `data` (text as UTF-8, or bytes) to `<dir>/<relative>` with mode 0600; creates parent folders 0700. */
export function saveEvidence(
  dir: string,
  relative: string,
  data: string | Buffer
): SavedFile {
  const bytes = typeof data === 'string' ? Buffer.from(data, 'utf8') : data;
  const path = join(dir, relative);
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  writeFileSync(path, bytes, { mode: 0o600 });
  chmodSync(path, 0o600);
  return {
    file: relative,
    bytes: bytes.length,
    sha256: createHash('sha256').update(bytes).digest('hex'),
  };
}

export function saveIndex(dir: string, name: string, rows: unknown): string {
  saveEvidence(dir, name, `${JSON.stringify(rows, null, 2)}\n`);
  return join(dir, name);
}

/** A file-name-safe form of `name`: no path parts, no leading dot, only letters, digits, `.`, `_`, `-`. */
export function sanitizeName(name: string, fallback = 'file'): string {
  const base = name.split(/[\\/]/).pop() ?? '';
  const clean = base
    .replace(/[^\w.-]+/g, '_')
    .replace(/^[._-]+/, '')
    .slice(0, 100);
  return clean || fallback;
}

/** Navigates after the listeners attach and waits for the load event (bounded); returns whether it fired. */
export async function navigateAndWait(
  cdp: Session,
  url: string,
  timeoutMs = 15000
): Promise<{ loaded: boolean; errorText?: string }> {
  await cdp.send('Page.enable');
  let fired = false;
  let wake: () => void = () => {};
  const loaded = new Promise<void>(resolve => {
    wake = resolve;
  });
  const onLoad = () => {
    fired = true;
    wake();
  };
  cdp.on('Page.loadEventFired', onLoad);
  const timer = setTimeout(wake, timeoutMs);
  try {
    const nav: Payload = await cdp.send('Page.navigate', { url });
    if (nav.errorText) return { loaded: false, errorText: nav.errorText };
    await loaded;
  } finally {
    clearTimeout(timer);
    cdp.off('Page.loadEventFired', onLoad);
  }
  return { loaded: fired };
}

/** The first `MAX_STDOUT_ITEMS` rows as lines, then a disclosure that names the file holding every row. */
export function showRows<T>(
  rows: T[],
  line: (row: T) => string,
  label: string,
  where: string
) {
  for (const row of rows.slice(0, MAX_STDOUT_ITEMS)) out(line(row));
  if (rows.length > MAX_STDOUT_ITEMS)
    out(
      `[${label}] ${MAX_STDOUT_ITEMS} of ${rows.length} shown; every row is in ${where}`
    );
}
