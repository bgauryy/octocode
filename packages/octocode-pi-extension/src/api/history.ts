import { boundedText } from './bounded.js';
import { MAX_FRAME_BYTES } from './protocol.js';
import { contentText, isRecord } from '../shared/util.js';

/** `messages.list` page size: the default, and the most one page holds (a larger `limit` gets this many; `earlier` reaches the rest). */
export const HISTORY_PAGE = 20;
export const HISTORY_MAX_PAGE = 100;

/**
 * One page of the transcript's user and assistant text, oldest first: the `limit` messages before index `before`
 * (default: the end); a `limit` outside 1..`HISTORY_MAX_PAGE` is clamped to it and a `before` past the end means the end,
 * and the page says the size it used. Every message carries its `index`; `earlier.before` is the next page's `before` and is absent
 * on the first page of the transcript. A text over `RESULT_MAX_CHARS` is cut with its whole text saved to a file.
 */
export function historyPage(branch: readonly unknown[], lastAssistantText: string, limit: number, before?: number): unknown {
  const all: Array<{ role: string; text: string }> = [];
  for (const entry of branch) {
    if (!isRecord(entry) || entry['type'] !== 'message' || !isRecord(entry['message'])) continue;
    const { role, content } = entry['message'];
    if (role !== 'user' && role !== 'assistant') continue;
    const text = contentText(content, { separator: '' });
    if (text) all.push({ role, text });
  }
  const end = Math.min(before ?? all.length, all.length);
  let start = end;
  let bytes = 0;
  const messages: Array<{ index: number; role: string; text: string }> = [];
  for (let index = end - 1; index >= Math.max(end - limit, 0); index--) {
    const row = { index, role: all[index].role, text: boundedText(all[index].text) };
    const size = Buffer.byteLength(JSON.stringify(row)) + 1;
    if (messages.length > 0 && bytes + size > MAX_FRAME_BYTES / 2) break;
    messages.unshift(row);
    bytes += size;
    start = index;
  }
  return {
    messages,
    total: all.length,
    limit,
    ...(start > 0 ? { earlier: { before: start } } : {}),
    last: lastAssistantText ? boundedText(lastAssistantText) : null,
  };
}
