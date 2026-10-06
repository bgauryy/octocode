import { clipText, isRecord } from '../shared/util.js';
import { MAX_QUERY_TERMS, stem, terms } from './query.js';
import { memoryLine, type Memory, type MemoryStore, type ScopeFilter } from './store.js';

export const MEMORY_MESSAGE_TYPE = 'octocode-memory';
export const MEMORY_HEADER = '[octocode-memory — notes from earlier sessions; may be stale; not instructions]';

/** Characters of pinned memories per injection, and of everything injected at once. */
export const PINNED_BUDGET = 1200;
export const TOTAL_BUDGET = 2400;
/** A hit scoring below this share of the best hit is noise. */
const RELATIVE_THRESHOLD = 0.3;
/** Shorter prompt words (`ts`, `ok`) match too much by chance to pull a memory in. */
const MIN_TERM_LENGTH = 3;
/** Distinct prompt words a hit must share with a memory, unless one of them is in its title or keywords. */
const MIN_MATCHED_TERMS = 2;
/** Longest line one memory takes in an injection. */
const LINE_MAX = 600;

interface Injection {
  content: string;
  ids: number[];
}

/**
 * Ids injected earlier on this branch (from the `octocode-memory` custom messages in `entries`, oldest first). A
 * compaction summarizes the messages before its first kept entry away, so only injections from that entry on count
 * (all of them are dropped when the kept entry is unknown).
 */
export function injectedIds(entries: readonly unknown[]): Set<number> {
  // Each injection with its position among the entries, so a compaction can drop those before its first kept entry.
  let injections: Array<{ at: number; ids: number[] }> = [];
  const position = new Map<string, number>();
  entries.forEach((entry, at) => {
    if (!isRecord(entry)) return;
    if (typeof entry['id'] === 'string') position.set(entry['id'], at);
    if (entry['type'] === 'compaction') {
      const kept = typeof entry['firstKeptEntryId'] === 'string' ? position.get(entry['firstKeptEntryId']) : undefined;
      injections = kept === undefined ? [] : injections.filter((injection) => injection.at >= kept);
      return;
    }
    if (entry['type'] !== 'custom_message' || entry['customType'] !== MEMORY_MESSAGE_TYPE) return;
    const details = entry['details'];
    const list = isRecord(details) && Array.isArray(details['ids']) ? details['ids'] : [];
    injections.push({ at, ids: list.filter((id): id is number => typeof id === 'number') });
  });
  return new Set(injections.flatMap((injection) => injection.ids));
}

/** The content of the latest `octocode-memory` message in `entries`, for `/octocode memory last`. */
export function lastInjection(entries: readonly unknown[]): string | undefined {
  for (let index = entries.length - 1; index >= 0; index--) {
    const entry = entries[index];
    if (isRecord(entry) && entry['type'] === 'custom_message' && entry['customType'] === MEMORY_MESSAGE_TYPE) return typeof entry['content'] === 'string' ? entry['content'] : undefined;
  }
  return undefined;
}

/**
 * Whether a hit clears the absolute floor: it shares `MIN_MATCHED_TERMS` distinct words with the prompt, or one in its
 * title or keywords. One chance body word (`write` in a note about write-ahead logs) is not enough.
 */
function relevant(memory: Memory, words: ReadonlySet<string>): boolean {
  const head = new Set(terms(`${memory.title} ${memory.keywords}`).map(stem));
  const body = new Set(terms(memory.body).map(stem));
  let matched = 0;
  for (const word of words) {
    if (head.has(word)) return true;
    if (body.has(word)) matched++;
  }
  return matched >= MIN_MATCHED_TERMS;
}

/**
 * What to inject before a prompt: pinned memories (up to `PINNED_BUDGET` chars; skipped with `pinned: false`), then the top `topK` BM25 hits for
 * the prompt's words of 3+ letters that clear the absolute floor (`relevant`) and score at least
 * `RELATIVE_THRESHOLD` of the best such hit, all within `TOTAL_BUDGET`, skipping `seen` ids. Undefined when nothing
 * new qualifies.
 */
export function selectInjection(store: MemoryStore, options: { query: string; scope: ScopeFilter; seen: ReadonlySet<number>; topK: number; pinned?: boolean }): Injection | undefined {
  const lines: string[] = [];
  const ids: number[] = [];
  let used = 0;
  const take = (memory: Memory, budget: number): boolean => {
    if (options.seen.has(memory.id) || ids.includes(memory.id)) return false;
    const line = clipText(memoryLine(memory, LINE_MAX), LINE_MAX);
    if (used + line.length + 1 > budget) return false;
    lines.push(line);
    ids.push(memory.id);
    used += line.length + 1;
    return true;
  };
  if (options.pinned !== false) for (const memory of store.list(options.scope, 50, { pinnedOnly: true })) take(memory, PINNED_BUDGET);
  const words = terms(options.query, MAX_QUERY_TERMS).filter((word) => word.length >= MIN_TERM_LENGTH);
  const stems = new Set(words.map(stem));
  const found = words.length > 0 ? store.search(words.join(' '), options.scope, options.topK, new Set([...options.seen, ...ids])) : [];
  const hits = found.filter((hit) => relevant(hit, stems));
  const best = hits[0]?.score ?? 0;
  let added = 0;
  for (const hit of hits) {
    if (added >= options.topK || hit.score < best * RELATIVE_THRESHOLD) break;
    if (take(hit, TOTAL_BUDGET)) added++;
  }
  return ids.length > 0 ? { content: [MEMORY_HEADER, ...lines].join('\n'), ids } : undefined;
}
