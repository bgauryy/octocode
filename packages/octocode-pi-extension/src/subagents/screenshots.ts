import fs from 'node:fs';
import path from 'node:path';
import { globalPaths } from '../shared/home.js';
import { isRecord } from '../shared/util.js';

/**
 * A subagent's screenshots (the `browser` tool) live only in its own conversation. The parent saves the
 * latest ones next to the team database so the user sees what the agent sees, live in the tool row and as a path to open.
 */

/** Largest screenshot (base64 chars) drawn inline in the tool row; bigger ones are only saved. */
export const INLINE_MAX_CHARS = 700_000;
const KEEP_FILES = 30;

export interface Shot {
  path: string;
  /** Base64 data when small enough to draw inline. */
  data?: string;
  mimeType: string;
}

/** Image parts of a tool result (`result.content[]` with type "image"). */
export function imagesOf(result: unknown): Array<{ data: string; mimeType: string }> {
  const content = isRecord(result) && Array.isArray(result['content']) ? result['content'] : [];
  return content.flatMap((part) =>
    isRecord(part) && part['type'] === 'image' && typeof part['data'] === 'string' ? [{ data: part['data'], mimeType: typeof part['mimeType'] === 'string' ? part['mimeType'] : 'image/png' }] : [],
  );
}

export function shotsDir(env: NodeJS.ProcessEnv = process.env): string {
  return path.join(globalPaths(env).team, 'shots');
}

/** Write one screenshot and keep only the newest few files. */
export function saveShot(dir: string, agentId: string, index: number, image: { data: string; mimeType: string }): Shot {
  fs.mkdirSync(dir, { recursive: true });
  const extension = image.mimeType.includes('jpeg') ? 'jpg' : 'png';
  const file = path.join(dir, `${agentId}-${index}.${extension}`);
  fs.writeFileSync(file, Buffer.from(image.data, 'base64'));
  const files = fs
    .readdirSync(dir)
    // Another Pi process may prune the shared folder meanwhile: a vanished file simply sorts last.
    .map((name) => ({ name, mtime: fs.statSync(path.join(dir, name), { throwIfNoEntry: false })?.mtimeMs ?? 0 }))
    .sort((a, b) => b.mtime - a.mtime);
  for (const old of files.slice(KEEP_FILES)) fs.rmSync(path.join(dir, old.name), { force: true });
  return { path: file, mimeType: image.mimeType, ...(image.data.length <= INLINE_MAX_CHARS ? { data: image.data } : {}) };
}
