import fs from 'node:fs';
import path from 'node:path';
import { sha256 } from '../shared/atomic.js';
import { fenceUntrusted, headerText } from '../shared/fence.js';
import { USER_SENDER } from '../shared/messages.js';
import { saveNamedContent } from '../shared/spill.js';
import { cutText } from '../shared/util.js';
import { MESSAGE_MAX_CHARS } from './routing.js';

/**
 * The file a long message is saved to (`spilledMessage`): `message-<sha256 of its content>.txt` in the session's private
 * output folder. The name is the content's hash, so the message's signature (`team/proof.ts`), which covers the delivered
 * head and the pointer with this path, also covers the whole text: a recipient checks the file against its own name
 * (`spillIntact`) before it reads a verified message as the user's.
 */
const FILE_NAME = /^message-([0-9a-f]{64})\.txt$/;

/** The pointer `spilledMessage` ends a long message with: the path is the file's. */
const POINTER = /\n\[Full message \(\d+ characters\): (.+)\]$/;

/** Saves `content` under its hash and returns the path, or undefined when it could not be written. Never throws. */
export const saveMessageFile = (content: string): string | undefined => saveNamedContent(content, `message-${sha256(content)}.txt`);

/**
 * What goes to the readers of a message: `text` when it fits `MESSAGE_MAX_CHARS`; a longer one is saved whole to a
 * private file and delivered as its head plus the file's path, still within the limit so no reader cuts the pointer off.
 * `text` is already sanitized, and the file holds it the way the recipient's message would carry it: fenced as data from
 * `from` (the user's own words excepted), because the recipient reads the file with `read`, outside the message's fence.
 * The file is named by the hash of its content (above), so the signature of a user's message covers it. An error when the file cannot be written (nothing is sent rather than a message with a silent gap).
 */
export function spilledMessage(text: string, from: string): string | { error: string } {
  if (text.length <= MESSAGE_MAX_CHARS) return text;
  const file = saveMessageFile(from === USER_SENDER ? text : fenceUntrusted(`Full message (${text.length} characters) from ${headerText(from, 64)}`, text));
  if (!file) return { error: `Message is ${text.length} characters (the limit is ${MESSAGE_MAX_CHARS}) and could not be saved to a file. Send a summary. Nothing was sent.` };
  const note = `\n[Full message (${text.length} characters): ${file}]`;
  return `${cutText(text, Math.max(0, MESSAGE_MAX_CHARS - note.length), { chars: true })}${note}`;
}

/** A file this recently changed may change again within the same timestamp tick, unseen by its stat: it is read every time. */
const SETTLED_MS = 1_000;
const REMEMBERED_FILES = 64;

/** Files that held their hash, by path (the name carries the hash), with the stat they had: intact until the stat moves. */
const intact = new Map<string, string>();

const statKey = (stat: fs.Stats): string => `${stat.dev}:${stat.ino}:${stat.size}:${stat.mtimeMs}:${stat.ctimeMs}`;

/**
 * Whether the saved file a delivered message points at still holds what was saved: true for a message with no pointer; false
 * when the file is missing, changed, or not named by a content hash (a pointer this version did not write). A file that held
 * its hash is read and hashed once more only when its stat (inode, size, modification and change time) moves, so a message
 * offered every tick costs one stat; a refusal is never remembered. `now`: the time, in ms.
 */
export function spillIntact(text: string, now = Date.now()): boolean {
  const file = POINTER.exec(text)?.[1];
  if (file === undefined) return true;
  const hash = FILE_NAME.exec(path.basename(file))?.[1];
  if (hash === undefined) return false;
  try {
    const before = fs.statSync(file);
    const key = statKey(before);
    if (intact.get(file) === key) return true;
    intact.delete(file);
    if (sha256(fs.readFileSync(file)) !== hash) return false;
    // Remembered only if the file did not move while it was read, and is old enough for its stat to show a later change.
    if (now - before.ctimeMs >= SETTLED_MS && statKey(fs.statSync(file)) === key) {
      intact.set(file, key);
      if (intact.size > REMEMBERED_FILES) intact.delete(intact.keys().next().value!);
    }
    return true;
  } catch {
    return false;
  }
}
