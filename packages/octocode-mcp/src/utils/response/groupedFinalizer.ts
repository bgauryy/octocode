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

export type PerQueryPagination = CharPagination & {
  id: string;
};

export type QueryWithPagination = {
  id?: unknown;
  charLength?: unknown;
  charOffset?: unknown;
};

type ItemPaginationConfig<TGroup, TItem> = {
  groups: TGroup[];
  getItems: (group: TGroup) => readonly TItem[];
  setItems: (group: TGroup, items: TItem[]) => TGroup;
  charOffset: number;
  charLength: number;
};

type GroupPaginationConfig<TGroup, TItem> = ItemPaginationConfig<
  TGroup,
  TItem
> & {
  truncateOversizedItem?: (
    item: TItem,
    charLength: number,
    group: TGroup
  ) => TItem;
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

export function paginateNestedItems<TGroup, TItem>({
  groups,
  getItems,
  setItems,
  charOffset,
  charLength,
}: ItemPaginationConfig<TGroup, TItem>): {
  groups: TGroup[];
  pagination: CharPagination;
} {
  type Cell = {
    groupIndex: number;
    item: TItem | null;
    start: number;
    end: number;
  };

  const cells: Cell[] = [];
  let cursor = 0;
  groups.forEach((group, groupIndex) => {
    const items = getItems(group);
    if (items.length === 0) {
      const size = countSerializedChars(setItems(group, []));
      cells.push({
        groupIndex,
        item: null,
        start: cursor,
        end: cursor + size,
      });
      cursor += size;
      return;
    }

    for (const item of items) {
      const size = countSerializedChars(item);
      cells.push({ groupIndex, item, start: cursor, end: cursor + size });
      cursor += size;
    }
  });

  const totalChars = cursor;
  const start = Math.max(0, charOffset);
  const end = start + charLength;
  const selectedByGroup = new Map<number, TItem[]>();
  let lastConsumedEnd = start;

  for (const cell of cells) {
    if (cell.start < start) continue;
    if (cell.start >= end) break;
    const bucket = selectedByGroup.get(cell.groupIndex) ?? [];
    if (cell.item) bucket.push(cell.item);
    selectedByGroup.set(cell.groupIndex, bucket);
    lastConsumedEnd = cell.end;
  }

  const selectedGroups = Array.from(selectedByGroup.entries()).map(
    ([groupIndex, items]) => setItems(groups[groupIndex]!, items)
  );

  return {
    groups: selectedGroups,
    pagination: buildCharPagination(
      charOffset,
      charLength,
      Math.max(0, lastConsumedEnd - start),
      totalChars
    ),
  };
}

export function paginateGroupsWithNestedItemEscape<TGroup, TItem>({
  groups,
  getItems,
  setItems,
  charOffset,
  charLength,
  truncateOversizedItem,
}: GroupPaginationConfig<TGroup, TItem>): {
  groups: TGroup[];
  pagination: CharPagination;
} {
  const sizes = groups.map(group => countSerializedChars(group));
  let cursor = 0;
  const offsets = sizes.map(size => {
    const offset = { start: cursor, end: cursor + size };
    cursor += size;
    return offset;
  });
  const totalChars = cursor;
  const start = Math.max(0, charOffset);
  const firstIndex = groups.findIndex(
    (_, index) => offsets[index]!.start >= start
  );
  const safeLength = Math.max(charLength, 1);

  if (firstIndex === -1) {
    return {
      groups: [],
      pagination: buildCharPagination(charOffset, charLength, 0, totalChars),
    };
  }

  const firstSize = sizes[firstIndex]!;
  if (firstSize > 2 * safeLength) {
    const oversized = groups[firstIndex]!;
    const offsetWithinGroup = Math.max(0, start - offsets[firstIndex]!.start);
    let sliced = paginateNestedItems({
      groups: [oversized],
      getItems,
      setItems,
      charOffset: offsetWithinGroup,
      charLength,
    });
    let consumed = sliced.pagination.charLength;

    if (consumed > 2 * safeLength && truncateOversizedItem) {
      const oversizedGroup = sliced.groups[0];
      const firstItem = oversizedGroup
        ? getItems(oversizedGroup)[0]
        : undefined;
      if (firstItem) {
        const truncated = truncateOversizedItem(
          firstItem,
          safeLength,
          oversizedGroup!
        );
        const rest = oversizedGroup ? getItems(oversizedGroup).slice(1) : [];
        const nextGroup = setItems(oversizedGroup!, [truncated, ...rest]);
        sliced = {
          groups: [nextGroup],
          pagination: sliced.pagination,
        };
        consumed = countSerializedChars(nextGroup);
      }
    }

    return {
      groups: sliced.groups,
      pagination: buildCharPagination(
        charOffset,
        charLength,
        consumed,
        totalChars
      ),
    };
  }

  const selected: TGroup[] = [];
  let consumed = 0;
  for (let index = firstIndex; index < groups.length; index += 1) {
    const size = sizes[index]!;
    if (selected.length > 0 && consumed + size > charLength) break;
    selected.push(groups[index]!);
    consumed += size;
    if (consumed >= charLength) break;
  }

  return {
    groups: selected,
    pagination: buildCharPagination(
      charOffset,
      charLength,
      consumed,
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
): Array<{ id: string; error: string; status?: number }> {
  const errors: Array<{ id: string; error: string; status?: number }> = [];
  for (const result of results) {
    if (result.status !== 'error') continue;
    const { message, status } = unwrapProviderError(
      (result.data as { error?: unknown }).error
    );
    errors.push({
      id: result.id,
      error: message,
      ...(status !== undefined ? { status } : {}),
    });
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
    structuredContent: sanitizeStructuredContent(responseData) as T,
    text,
    isError,
  };
}
