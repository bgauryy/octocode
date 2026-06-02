import {
  createResponseFormat,
  sanitizeStructuredContent,
} from '../../responses.js';
import type { BulkFinalizerOutput } from '../../types/bulk.js';
import type { FlatQueryResult } from '../../types/toolResults.js';
import { countSerializedChars } from './charSavings.js';
import { getBulkDefaultCharLength } from '../pagination/charLimit.js';

export type CharPagination = {
  currentPage: number;
  totalPages: number;
  hasMore: boolean;
  charOffset: number;
  charLength: number;
  totalChars: number;
};

export type PerQueryPagination = CharPagination & {
  id: string;
};

export type QueryWithPagination = {
  id?: unknown;
  charLength?: unknown;
  charOffset?: unknown;
};

/**
 * Configuration for {@link paginateGroupsCharWindow} — the single grouped
 * paginator. `getItemText`/`setItemText` are OPTIONAL: when supplied they name
 * the one paginatable text field of an item (e.g. a code match's `value` or a
 * file's `content`) so an oversized item is *windowed* across pages instead of
 * truncated. Items without a text field (e.g. directory entries) are treated as
 * atomic.
 */
type CharWindowConfig<TGroup, TItem> = {
  groups: TGroup[];
  getItems: (group: TGroup) => readonly TItem[];
  setItems: (group: TGroup, items: TItem[]) => TGroup;
  getItemText?: (item: TItem) => string | undefined;
  setItemText?: (item: TItem, text: string) => TItem;
  charOffset: number;
  charLength: number;
  /**
   * Optional WHOLE-ITEM page cap (`itemsPerPage`). When set, at most this many
   * items are emitted across the page regardless of the char budget — the item
   * is the atomic unit. The char window still applies as a backstop (whichever
   * binds first wins), and an item whose own text exceeds the budget is still
   * windowed via getItemText. When the item cap is the binding constraint the
   * cursor advances to the next item boundary so the next call resumes cleanly.
   */
  maxItems?: number;
};

function readNumber(
  value: unknown,
  predicate: (n: number) => boolean
): number | undefined {
  if (typeof value !== 'number' || !Number.isFinite(value)) return undefined;
  return predicate(value) ? value : undefined;
}

export function readPositiveNumber(value: unknown): number | undefined {
  return readNumber(value, n => n > 0);
}

export function readNonNegativeNumber(value: unknown): number | undefined {
  return readNumber(value, n => n >= 0);
}

function buildCharPagination(
  charOffset: number,
  requestedLength: number,
  consumedLength: number,
  totalChars: number
): CharPagination {
  const safeRequested = Math.max(requestedLength, 1);
  const safeConsumed = Math.max(consumedLength, 0);
  const safeTotal = Math.max(totalChars, 0);
  const totalPages = Math.max(1, Math.ceil(safeTotal / safeRequested));
  const clampedOffset =
    safeTotal === 0
      ? 0
      : Math.min(Math.max(charOffset, 0), Math.max(safeTotal - 1, 0));
  const currentPage =
    safeTotal === 0
      ? 1
      : Math.min(totalPages, Math.floor(clampedOffset / safeRequested) + 1);
  return {
    currentPage,
    totalPages,
    hasMore: charOffset + safeConsumed < safeTotal,
    charOffset,
    charLength: safeConsumed,
    totalChars: safeTotal,
  };
}

/**
 * Single char-accurate windowing paginator for grouped tool output.
 *
 * Replaces the former `paginateNestedItems` + `paginateGroupsWithNestedItemEscape`
 * + `truncateOversizedItem` trio. There is NO truncation: an item whose
 * paginatable text field (`getItemText`) is larger than the page budget is
 * SLICED to fit, and the remainder is reached by advancing `charOffset` on the
 * next call — pure pagination, with no "… [truncated]" marker and no
 * out-of-band recovery warning pointing at a different tool. The former magic
 * envelope constants (`-64` / `-128`) are replaced by a COMPUTED per-item
 * envelope (`countSerializedChars` of the item with its text field emptied).
 *
 * Guarantees:
 *  - The serialized page never exceeds `charLength` by more than a single
 *    group's wrapper envelope (id/owner/repo + braces) — never by item
 *    content, and never the old ≤2× overflow.
 *  - `totalChars` is computed from a wrapper+item cell model that BOTH the
 *    per-query and bulk callers use, so the two layers always agree (no
 *    wrapper-undercount drift).
 *  - Forward progress: an item whose envelope alone exceeds the budget is still
 *    emitted (text sliced to whatever fits, possibly empty) and the cursor
 *    advances, so the agent is never stuck on a zero-progress page.
 */
export function paginateGroupsCharWindow<TGroup, TItem>({
  groups,
  getItems,
  setItems,
  getItemText,
  setItemText,
  charOffset,
  charLength,
  maxItems,
}: CharWindowConfig<TGroup, TItem>): {
  groups: TGroup[];
  pagination: CharPagination;
} {
  // Narrow the optional accessor pair ONCE into a single non-null object, so
  // the text-slicing paths below need no `!` non-null assertions.
  const textAccessors =
    getItemText && setItemText ? { get: getItemText, set: setItemText } : null;

  type ItemCell = {
    groupIndex: number;
    item: TItem;
    /** Absolute char offset where this item's paginatable text begins. */
    textStart: number;
    textLen: number;
    start: number;
    end: number;
  };

  // Build the full ordered cell stream: each group contributes a wrapper cost
  // (its envelope with zero items — this is what counts id/owner/repo so the
  // total matches the bulk serialization) plus one cell per item.
  const cells: ItemCell[] = [];
  let cursor = 0;

  groups.forEach((group, groupIndex) => {
    cursor += countSerializedChars(setItems(group, []));

    for (const item of getItems(group)) {
      const text = textAccessors?.get(item);
      if (textAccessors && text !== undefined) {
        const envelope = countSerializedChars(textAccessors.set(item, ''));
        const textLen = text.length;
        const start = cursor;
        const textStart = start + envelope;
        const end = textStart + textLen;
        cells.push({ groupIndex, item, textStart, textLen, start, end });
        cursor = end;
      } else {
        const size = countSerializedChars(item);
        const start = cursor;
        cells.push({
          groupIndex,
          item,
          textStart: start,
          textLen: 0,
          start,
          end: start + size,
        });
        cursor = start + size;
      }
    }
  });

  const totalChars = cursor;
  const safeLength = Math.max(charLength, 1);
  const start = Math.min(Math.max(charOffset, 0), totalChars);
  const end = Math.min(start + safeLength, totalChars);

  const sliceText = (cell: ItemCell): TItem => {
    if (cell.textLen === 0 || !textAccessors) return cell.item;
    const textFrom = Math.min(
      Math.max(start - cell.textStart, 0),
      cell.textLen
    );
    const textTo = Math.min(Math.max(end - cell.textStart, 0), cell.textLen);
    if (textFrom === 0 && textTo === cell.textLen) return cell.item;
    const full = textAccessors.get(cell.item) ?? '';
    return textAccessors.set(cell.item, full.slice(textFrom, textTo));
  };

  const selectedByGroup = new Map<number, TItem[]>();
  let consumedEnd = start;
  let itemsSelected = 0;
  const itemCap =
    typeof maxItems === 'number' && maxItems > 0 ? maxItems : undefined;

  for (const cell of cells) {
    if (cell.end <= start) continue; // fully before the window
    if (cell.start >= end) break; // fully after the window
    const bucket = selectedByGroup.get(cell.groupIndex) ?? [];
    bucket.push(sliceText(cell));
    selectedByGroup.set(cell.groupIndex, bucket);
    consumedEnd = Math.max(consumedEnd, Math.min(cell.end, end));
    itemsSelected += 1;
    // Whole-item page cap: stop after `itemsPerPage` items and park the cursor
    // at this item's end so the next call resumes at the following item. Only
    // binds when it cuts BEFORE the char window would have — `hasMore` then
    // falls out of `consumedEnd < totalChars` in buildCharPagination.
    if (itemCap !== undefined && itemsSelected >= itemCap) {
      consumedEnd = Math.max(consumedEnd, cell.end);
      break;
    }
  }

  // Forward-progress backstop: the window landed in a gap (e.g. the requested
  // offset skipped past every item that overlaps `[start, end)`) but more data
  // exists. Pull in the first item at/after `start` so the cursor advances and
  // the agent is never stuck re-requesting the same empty page.
  if (selectedByGroup.size === 0 && start < totalChars) {
    const next = cells.find(cell => cell.end > start);
    if (next) {
      selectedByGroup.set(next.groupIndex, [sliceText(next)]);
      consumedEnd = Math.max(consumedEnd, Math.min(next.end, end));
    }
  }

  // Iterate groups in their natural order and pull the selected items from the
  // map — this preserves order without a sort and uses each `group` directly,
  // so there is no `groups[index]!` array-access assertion.
  const selectedGroups: TGroup[] = [];
  groups.forEach((group, groupIndex) => {
    const items = selectedByGroup.get(groupIndex);
    if (items) selectedGroups.push(setItems(group, items));
  });

  return {
    groups: selectedGroups,
    pagination: buildCharPagination(
      start,
      safeLength,
      Math.max(0, consumedEnd - start),
      totalChars
    ),
  };
}

/**
 * Bulk char-window policy shared by every grouped finalizer (search_code,
 * fetch_content, …). Auto-paginates the merged groups at the count-scaled
 * {@link getBulkDefaultCharLength} (one base window reserved per group, so a
 * large first group can't starve its siblings off page 1 — #3), adopting the
 * slice ONLY when the caller drove pagination (explicit
 * `responseCharOffset`/`responseCharLength`) OR the response actually
 * overflowed — so a response that fits is emitted whole with no pagination
 * noise. Centralizing this here keeps the "explicit-or-overflow" rule in one
 * place instead of copy-pasted per finalizer (drift risk).
 */
export function applyBulkCharWindow<TGroup, TItem>(
  groups: TGroup[],
  config: {
    responseCharOffset?: number;
    responseCharLength?: number;
    maxItems?: number;
  },
  accessors: {
    getItems: (group: TGroup) => readonly TItem[];
    setItems: (group: TGroup, items: TItem[]) => TGroup;
    getItemText?: (item: TItem) => string | undefined;
    setItemText?: (item: TItem, text: string) => TItem;
  }
): { groups: TGroup[]; responsePagination?: CharPagination } {
  if (groups.length === 0) return { groups };
  const explicitlyPaginated =
    config.responseCharLength !== undefined ||
    config.responseCharOffset !== undefined;
  const sliced = paginateGroupsCharWindow({
    groups,
    ...accessors,
    charOffset: config.responseCharOffset ?? 0,
    // Reserve one base window per group so a large first group doesn't starve
    // its siblings off page 1 (#3). Explicit responseCharLength still wins.
    charLength:
      config.responseCharLength ?? getBulkDefaultCharLength(groups.length),
    maxItems: config.maxItems,
  });
  // The item cap (itemsPerPage) is a page-shaping constraint, not just an
  // overflow backstop: adopt the slice when it bound (hasMore) even if the
  // caller didn't pass explicit char cursors.
  return explicitlyPaginated || sliced.pagination.hasMore
    ? { groups: sliced.groups, responsePagination: sliced.pagination }
    : { groups };
}

export function dedupeHints(hints: readonly string[]): string[] {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const hint of hints) {
    if (typeof hint === 'string' && hint.trim().length > 0 && !seen.has(hint)) {
      seen.add(hint);
      result.push(hint);
    }
  }
  return result;
}

/**
 * Unwrap an `error` field that may be either a plain string or a
 * `GitHubAPIError`-shaped object `{ error: string, status?: number, ... }`.
 * Returning `status` separately lets finalizers route HTTP semantics into
 * dynamic error hints even when the provider failed to supply a textual
 * reason (the generic "Provider error" path).
 */
function unwrapProviderError(value: unknown): {
  message: string;
  status?: number;
} {
  if (typeof value === 'string') return { message: value };
  if (typeof value === 'object' && value !== null) {
    const obj = value as { error?: unknown; status?: unknown };
    const message =
      typeof obj.error === 'string' && obj.error.length > 0
        ? obj.error
        : 'Provider error';
    const status =
      typeof obj.status === 'number' && Number.isFinite(obj.status)
        ? obj.status
        : undefined;
    return { message, status };
  }
  return { message: 'Provider error' };
}

export function collectFlatErrors(
  results: readonly FlatQueryResult[]
): Array<{ id: string; error: string }> {
  const errors: Array<{ id: string; error: string }> = [];
  for (const result of results) {
    if (result.status !== 'error') continue;
    const { message, status } = unwrapProviderError(
      (result.data as { error?: unknown }).error
    );
    // Embed HTTP status in the message string — the output schema for grouped
    // tools (e.g. githubSearchCode) uses additionalProperties:false on error
    // items, so a separate `status` property fails MCP schema validation.
    const errorMessage =
      status !== undefined ? `${message} (HTTP ${status})` : message;
    errors.push({ id: result.id, error: errorMessage });
  }
  return errors;
}

/**
 * Serialize + sanitize a finalizer response.  Generic over `T` so callers can
 * pin the structured-content type to their registered output schema —
 * `formatFinalizedResponse<z.infer<typeof MySchema>>(...)` — and get the
 * compile-time guard provided by {@link BulkFinalizerOutput}.  The
 * `Record<string, unknown>` constraint matches the MCP SDK boundary so the
 * bulk runner can return the result without an `as` cast.
 */
export function formatFinalizedResponse<T extends Record<string, unknown>>(
  responseData: T,
  keysPriority: readonly string[],
  isError?: boolean
): BulkFinalizerOutput<T> {
  const text = createResponseFormat(
    responseData as Parameters<typeof createResponseFormat>[0],
    [...keysPriority]
  );

  return {
    // structuredContent holds the canonical records, surfaced identically in
    // `text` and here. (#A1)
    structuredContent: sanitizeStructuredContent(responseData) as T,
    text,
    isError,
  };
}
