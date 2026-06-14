import {
  createResponseFormat,
  sanitizeStructuredContent,
} from '../../responses.js';
import type { BulkFinalizerOutput } from '../../types/bulk.js';
import type { FlatQueryResult } from '../../types/toolResults.js';
import { countSerializedChars } from './charSavings.js';

export type CharPagination = {
  currentPage: number;
  totalPages: number;
  hasMore: boolean;
  charOffset: number;
  charLength: number;
  totalChars: number;
};

export type QueryWithPagination = {
  id?: unknown;
  charLength?: unknown;
  charOffset?: unknown;
};

type CharWindowConfig<TGroup, TItem> = {
  groups: TGroup[];
  getItems: (group: TGroup) => readonly TItem[];
  setItems: (group: TGroup, items: TItem[]) => TGroup;
  getItemText?: (item: TItem) => string | undefined;
  setItemText?: (item: TItem, text: string) => TItem;
  charOffset: number;
  charLength: number;

  maxItems?: number;
};

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
  const textAccessors =
    getItemText && setItemText ? { get: getItemText, set: setItemText } : null;

  type ItemCell = {
    groupIndex: number;
    item: TItem;

    textStart: number;
    textLen: number;
    start: number;
    end: number;
  };

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
    if (cell.end <= start) continue;
    if (cell.start >= end) break;
    const bucket = selectedByGroup.get(cell.groupIndex) ?? [];
    bucket.push(sliceText(cell));
    selectedByGroup.set(cell.groupIndex, bucket);
    consumedEnd = Math.max(consumedEnd, Math.min(cell.end, end));
    itemsSelected += 1;
    if (itemCap !== undefined && itemsSelected >= itemCap) {
      consumedEnd = Math.max(consumedEnd, cell.end);
      break;
    }
  }

  if (selectedByGroup.size === 0 && start < totalChars) {
    const next = cells.find(cell => cell.end > start);
    if (next) {
      selectedByGroup.set(next.groupIndex, [sliceText(next)]);
      consumedEnd = Math.max(consumedEnd, Math.min(next.end, end));
    }
  }

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
): Array<{ id: string; error: string; hints?: string[] }> {
  const errors: Array<{ id: string; error: string; hints?: string[] }> = [];
  for (const result of results) {
    if (result.status !== 'error') continue;
    const { message, status } = unwrapProviderError(
      (result.data as { error?: unknown }).error
    );
    const errorMessage =
      status !== undefined ? `${message} (HTTP ${status})` : message;
    const hints = Array.isArray(result.data.hints)
      ? result.data.hints.filter(
          (hint): hint is string =>
            typeof hint === 'string' && hint.trim().length > 0
        )
      : undefined;
    errors.push({
      id: result.id,
      error: errorMessage,
      ...(hints && hints.length > 0 ? { hints } : {}),
    });
  }
  return errors;
}

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
    structuredContent: sanitizeStructuredContent(responseData) as T,
    text,
    isError,
  };
}
