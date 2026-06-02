import { TOOL_NAMES } from '../../tools/toolMetadata/proxies.js';
import {
  getOutputCharLimit,
  getBulkDefaultCharLength,
} from '../pagination/charLimit.js';
import type { BulkToolResponse } from '../../types/bulk.js';
import type {
  FlatQueryResult,
  PaginationInfo,
} from '../../types/toolResults.js';
// Hard ceiling for the auto-pagination default. Even if the deployment config
// sets a very high `output.pagination.defaultCharLength`, a single aggregated
// bulk response must never exceed the documented max single-response budget —
// otherwise one large query (e.g. a fullContent PR with many files) sails past
// the MCP client's token limit and is truncated/spilled wholesale instead of
// being paginated with a cursor. Mirrors LOCAL_OVERLAY_MAX_CHAR_LENGTH. (#T2)
const MAX_DEFAULT_OUTPUT_CHAR_LENGTH = 100_000;
const FALLBACK_EXCLUDED_FIELDS = new Set([
  'hints',
  'warnings',
  'pagination',
  'outputPagination',
  'charPagination',
  'responsePagination',
]);

/**
 * localFindFiles stores its per-query char-pagination result as `charPagination`
 * (the name used by the upstream `@octocodeai/octocode-core` type). The bulk
 * engine tracks char cursors under the canonical key `outputPagination`. This
 * helper promotes the field so both names are present and evidence / hints
 * builders can use the standard path.
 *
 * If `outputPagination` is already set (e.g. from a previous promotion) the
 * data object is returned unchanged.
 */
function promoteCharPagination(
  data: Record<string, unknown>
): Record<string, unknown> {
  if (!data.charPagination || data.outputPagination) return data;
  return { ...data, outputPagination: data.charPagination };
}

interface PaginationRequest {
  offset?: number;
  length?: number;
}

interface ResolvedPaginationRequest {
  offset: number;
  length: number;
  explicit: boolean;
}

interface ValuePageResult<T> {
  value: T;
  actualOffset: number;
  pageEnd: number;
  totalChars: number;
  paginated: boolean;
}

interface CollectionConfig {
  field: string;
  kind: 'array' | 'record';
  itemPaginator?: (
    value: unknown,
    request: ResolvedPaginationRequest
  ) => ValuePageResult<unknown> | null;
}

interface CollectionSegment {
  field: string;
  kind: 'array' | 'record';
  key?: string;
  value: unknown;
  start: number;
  end: number;
  itemPaginator?: CollectionConfig['itemPaginator'];
}

function getDefaultCharLength(): number {
  // One pagination limit for every flow. Clamp to the hard ceiling so a high
  // deployment default can't produce an un-paginated overflow response. (#T2)
  return Math.min(
    Math.max(getOutputCharLimit(), 1),
    MAX_DEFAULT_OUTPUT_CHAR_LENGTH
  );
}

function resolveRequest(
  request: PaginationRequest,
  defaultLength: number = getDefaultCharLength()
): ResolvedPaginationRequest {
  return {
    offset: request.offset ?? 0,
    length: Math.max(request.length ?? defaultLength, 1),
    // `explicit` reflects ONLY whether the caller drove pagination — a
    // count-scaled default does not make the request explicit.
    explicit: request.offset !== undefined || request.length !== undefined,
  };
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return (
    typeof value === 'object' &&
    value !== null &&
    !Array.isArray(value) &&
    Object.getPrototypeOf(value) === Object.prototype
  );
}

function serialize(value: unknown): string {
  return JSON.stringify(value);
}

function createOutputPagination(
  charOffset: number,
  charLength: number,
  totalChars: number,
  pageSize: number
): PaginationInfo {
  const safePageSize = Math.max(pageSize, 1);
  const safeTotalChars = Math.max(totalChars, 0);
  // Uniform-page estimate. Real pages can consume MORE than pageSize when a
  // single atomic item (a fullContent PR diff, a huge match) exceeds it, so
  // this is only an upper bound mid-stream.
  const estimatedPages = Math.max(1, Math.ceil(safeTotalChars / safePageSize));
  const maxLogicalOffset =
    safeTotalChars === 0 ? 0 : Math.max(safeTotalChars - 1, 0);
  const pageOffset = Math.min(Math.max(charOffset, 0), maxLogicalOffset);
  const currentPage =
    safeTotalChars === 0
      ? 1
      : Math.min(estimatedPages, Math.floor(pageOffset / safePageSize) + 1);
  const hasMore = charOffset + charLength < safeTotalChars;

  return {
    currentPage,
    // Pin the count to the truth at the boundary: on the LAST page (nothing
    // more) the total is exactly the current page — an oversized item that ate
    // >pageSize otherwise leaves the uniform estimate overcounting (e.g.
    // "1/2" for a single page that fit everything). When more remains, the
    // count is at least currentPage+1.
    totalPages: hasMore
      ? Math.max(estimatedPages, currentPage + 1)
      : currentPage,
    hasMore,
    charOffset,
    charLength,
    totalChars: safeTotalChars,
  };
}

function withPaginationHints(
  data: Record<string, unknown>,
  pagination: PaginationInfo,
  kind: 'output' | 'response',
  options: { autoPaginated: boolean; requestedLength: number }
): Record<string, unknown> {
  const existingHints = Array.isArray(data.hints)
    ? data.hints.filter((hint): hint is string => typeof hint === 'string')
    : [];
  const charOffset = pagination.charOffset ?? 0;
  const charLength = pagination.charLength ?? 0;
  const totalChars = pagination.totalChars ?? 0;
  const nextOffset = charOffset + charLength;
  const pageSummaryHint = `Page ${pagination.currentPage}/${pagination.totalPages} (${charLength} of ${totalChars} chars)`;
  const continuationHint =
    kind === 'response'
      ? `Use responseCharOffset=${nextOffset} to continue this paginated bulk response.`
      : `Use charOffset=${nextOffset} to continue this paginated result.`;
  const autoPaginationHint = `Auto-paginated: Output (${totalChars} chars) exceeds ${options.requestedLength} char limit.`;
  const nextHints = [...existingHints];

  if (
    options.autoPaginated &&
    pagination.hasMore &&
    !nextHints.includes(autoPaginationHint)
  ) {
    nextHints.push(autoPaginationHint);
  }

  if (!nextHints.includes(pageSummaryHint)) {
    nextHints.push(pageSummaryHint);
  }

  if (pagination.hasMore && !nextHints.includes(continuationHint)) {
    nextHints.push(continuationHint);
  }

  return {
    ...data,
    hints: nextHints,
  };
}

function buildCollectionSegments(
  target: Record<string, unknown>,
  configs: CollectionConfig[]
): {
  baseObject: Record<string, unknown>;
  baseChars: number;
  segments: CollectionSegment[];
  totalChars: number;
} {
  const baseObject = { ...target };
  delete baseObject.outputPagination;
  delete baseObject.charPagination;

  for (const config of configs) {
    if (config.kind === 'array' && Array.isArray(baseObject[config.field])) {
      baseObject[config.field] = [];
    }
    if (config.kind === 'record' && isPlainObject(baseObject[config.field])) {
      baseObject[config.field] = {};
    }
  }

  const baseChars = serialize(baseObject).length;
  const segments: CollectionSegment[] = [];
  let currentOffset = baseChars;

  for (const config of configs) {
    const value = target[config.field];

    if (config.kind === 'array' && Array.isArray(value) && value.length > 0) {
      let firstItem = true;
      for (const item of value) {
        const serializedValue = serialize(item);
        const segmentLength = `${firstItem ? '' : ','}${serializedValue}`
          .length;
        segments.push({
          field: config.field,
          kind: 'array',
          value: item,
          start: currentOffset,
          end: currentOffset + segmentLength,
          itemPaginator: config.itemPaginator,
        });
        currentOffset += segmentLength;
        firstItem = false;
      }
      continue;
    }

    if (config.kind === 'record' && isPlainObject(value)) {
      let firstItem = true;
      for (const [key, itemValue] of Object.entries(value)) {
        const serializedValue = serialize(itemValue);
        const segmentLength =
          `${firstItem ? '' : ','}${serialize(key)}:${serializedValue}`.length;
        segments.push({
          field: config.field,
          kind: 'record',
          key,
          value: itemValue,
          start: currentOffset,
          end: currentOffset + segmentLength,
          itemPaginator: config.itemPaginator,
        });
        currentOffset += segmentLength;
        firstItem = false;
      }
    }
  }

  return {
    baseObject,
    baseChars,
    segments,
    totalChars: currentOffset,
  };
}

function materializeSegments(
  baseObject: Record<string, unknown>,
  segments: CollectionSegment[]
): Record<string, unknown> {
  const nextValue = { ...baseObject };

  for (const segment of segments) {
    if (segment.kind === 'array') {
      const existing = Array.isArray(nextValue[segment.field])
        ? [...(nextValue[segment.field] as unknown[])]
        : [];
      existing.push(segment.value);
      nextValue[segment.field] = existing;
      continue;
    }

    const existing = isPlainObject(nextValue[segment.field])
      ? { ...(nextValue[segment.field] as Record<string, unknown>) }
      : {};
    if (segment.key !== undefined) {
      existing[segment.key] = segment.value;
    }
    nextValue[segment.field] = existing;
  }

  return nextValue;
}

function paginateSegments(
  baseChars: number,
  totalChars: number,
  segments: CollectionSegment[],
  request: ResolvedPaginationRequest
): {
  selectedSegments: CollectionSegment[];
  actualOffset: number;
  pageEnd: number;
} {
  const firstSegmentIndex = segments.findIndex(
    segment => segment.end > request.offset
  );

  if (firstSegmentIndex === -1) {
    return {
      selectedSegments: [],
      actualOffset: request.offset,
      pageEnd: request.offset,
    };
  }

  const selectedSegments: CollectionSegment[] = [];
  let actualOffset: number | undefined;
  let pageEnd = 0;

  for (let index = firstSegmentIndex; index < segments.length; index += 1) {
    const segment = segments[index]!;
    const relativeOffset =
      index === firstSegmentIndex
        ? Math.max(0, request.offset - segment.start)
        : 0;
    const consumed = actualOffset === undefined ? 0 : pageEnd - actualOffset;
    const remaining = Math.max(request.length - consumed, 1);
    const segmentLength = segment.end - segment.start;

    if (relativeOffset === 0 && segmentLength <= remaining) {
      if (actualOffset === undefined) {
        actualOffset = request.offset < baseChars ? 0 : segment.start;
      }
      selectedSegments.push(segment);
      pageEnd = segment.end;
      continue;
    }

    if (segment.itemPaginator) {
      const partial = segment.itemPaginator(segment.value, {
        offset: relativeOffset,
        length: remaining,
        explicit: true,
      });

      if (partial) {
        if (actualOffset === undefined) {
          actualOffset =
            request.offset < baseChars
              ? 0
              : segment.start + partial.actualOffset;
        }
        selectedSegments.push({
          ...segment,
          value: partial.value,
        });
        pageEnd = segment.start + partial.pageEnd;
        // A partial only terminates the page when the LENGTH budget ran out
        // inside this segment. If the partial instead consumed the segment to
        // its end (a responseCharOffset that resumed MID-segment) and budget
        // still remains, fall through to the following segments — otherwise a
        // mid-segment resume returns only this one segment's tail and the
        // cursor stalls, never advancing into later queries (the multi-query
        // repo/PR/structure bulk "cursor stall" bug).
        const segmentFullyConsumed = partial.pageEnd >= partial.totalChars;
        const budgetRemains = pageEnd - actualOffset < request.length;
        if (segmentFullyConsumed && budgetRemains) {
          continue;
        }
        break;
      }
    }

    if (actualOffset === undefined) {
      actualOffset = request.offset < baseChars ? 0 : segment.start;
    }
    selectedSegments.push(segment);
    pageEnd = segment.end;
    break;
  }

  if (actualOffset === undefined) {
    actualOffset =
      request.offset < baseChars ? 0 : Math.min(request.offset, totalChars);
    pageEnd = Math.min(totalChars, Math.max(baseChars, actualOffset));
  }

  return {
    selectedSegments,
    actualOffset,
    pageEnd,
  };
}

function paginateConfiguredObjectValue(
  target: Record<string, unknown>,
  request: ResolvedPaginationRequest,
  configs: CollectionConfig[]
): ValuePageResult<Record<string, unknown>> {
  const fullChars = serialize(target).length;
  if (configs.length === 0) {
    return {
      value: target,
      actualOffset: 0,
      pageEnd: fullChars,
      totalChars: fullChars,
      paginated: false,
    };
  }

  const { baseObject, baseChars, segments, totalChars } =
    buildCollectionSegments(target, configs);

  if (
    segments.length === 0 ||
    (!request.explicit && totalChars <= request.length)
  ) {
    return {
      value: target,
      actualOffset: 0,
      pageEnd: totalChars,
      totalChars,
      paginated: false,
    };
  }

  if (request.offset >= totalChars) {
    return {
      value: baseObject,
      actualOffset: request.offset,
      pageEnd: request.offset,
      totalChars,
      paginated: true,
    };
  }

  const page = paginateSegments(baseChars, totalChars, segments, request);

  return {
    value: materializeSegments(baseObject, page.selectedSegments),
    actualOffset: page.actualOffset,
    pageEnd: page.pageEnd,
    totalChars,
    paginated: true,
  };
}

function paginateStringValue(
  value: string,
  request: ResolvedPaginationRequest
): ValuePageResult<string> {
  const codePoints = Array.from(value);
  const encodedLengths = codePoints.map(codePoint =>
    Math.max(JSON.stringify(codePoint).length - 2, 0)
  );
  const encodedTotal = encodedLengths.reduce((sum, length) => sum + length, 0);
  const totalChars = 2 + encodedTotal;

  if (!request.explicit && totalChars <= request.length) {
    return {
      value,
      actualOffset: 0,
      pageEnd: totalChars,
      totalChars,
      paginated: false,
    };
  }

  if (request.offset >= totalChars) {
    return {
      value: '',
      actualOffset: request.offset,
      pageEnd: request.offset,
      totalChars,
      paginated: true,
    };
  }

  let startIndex = 0;
  let startOffset = 0;

  if (request.offset > 1) {
    let prefix = 1;
    let found = false;
    for (let index = 0; index < encodedLengths.length; index += 1) {
      const length = encodedLengths[index]!;
      if (prefix + length > request.offset) {
        startIndex = index;
        startOffset = prefix;
        found = true;
        break;
      }
      prefix += length;
    }

    if (!found) {
      return {
        value: '',
        actualOffset: request.offset,
        pageEnd: request.offset,
        totalChars,
        paginated: true,
      };
    }
  }

  const availableBudget = Math.max(
    1,
    request.length - (startIndex === 0 ? 1 : 0) - 1
  );
  let encodedChunk = 0;
  let endIndex = startIndex;

  while (endIndex < codePoints.length) {
    const nextLength = encodedLengths[endIndex]!;
    if (encodedChunk + nextLength > availableBudget && endIndex > startIndex) {
      break;
    }
    encodedChunk += nextLength;
    endIndex += 1;
    if (encodedChunk >= availableBudget) {
      break;
    }
  }

  if (endIndex === startIndex && endIndex < codePoints.length) {
    encodedChunk += encodedLengths[endIndex]!;
    endIndex += 1;
  }

  const actualOffset = startIndex === 0 ? 0 : startOffset;
  const consumedLogicalChars =
    encodedChunk +
    (startIndex === 0 ? 1 : 0) +
    (endIndex === codePoints.length ? 1 : 0);

  return {
    value: codePoints.slice(startIndex, endIndex).join(''),
    actualOffset,
    pageEnd: Math.min(
      totalChars,
      actualOffset + Math.max(consumedLogicalChars, 1)
    ),
    totalChars,
    paginated: true,
  };
}

function paginateObjectFieldCore(
  target: Record<string, unknown>,
  field: string,
  request: ResolvedPaginationRequest,
  emptyValue: unknown,
  innerPaginate: (
    value: unknown,
    req: ResolvedPaginationRequest
  ) => ValuePageResult<unknown>
): ValuePageResult<Record<string, unknown>> | null {
  const baseValue = { ...target, [field]: emptyValue };
  delete baseValue.outputPagination;
  delete baseValue.charPagination;

  const wrapperChars = Math.max(serialize(baseValue).length - 2, 0);
  const innerPage = innerPaginate(target[field], {
    offset: request.offset <= wrapperChars ? 0 : request.offset - wrapperChars,
    length:
      request.offset <= wrapperChars
        ? Math.max(1, request.length - wrapperChars)
        : request.length,
    explicit: true,
  });
  const totalChars = wrapperChars + innerPage.totalChars;

  if (!request.explicit && totalChars <= request.length) {
    return {
      value: target,
      actualOffset: 0,
      pageEnd: totalChars,
      totalChars,
      paginated: false,
    };
  }

  if (request.offset >= totalChars) {
    return {
      value: baseValue,
      actualOffset: request.offset,
      pageEnd: request.offset,
      totalChars,
      paginated: true,
    };
  }

  return {
    value: {
      ...baseValue,
      [field]: innerPage.value,
    },
    actualOffset:
      request.offset <= wrapperChars
        ? 0
        : wrapperChars + innerPage.actualOffset,
    pageEnd: wrapperChars + innerPage.pageEnd,
    totalChars,
    paginated: true,
  };
}

function paginateObjectStringField(
  target: Record<string, unknown>,
  field: string,
  request: ResolvedPaginationRequest
): ValuePageResult<Record<string, unknown>> | null {
  if (typeof target[field] !== 'string') return null;
  return paginateObjectFieldCore(target, field, request, '', (v, r) =>
    paginateStringValue(v as string, r)
  );
}

function paginateNestedObjectField(
  target: Record<string, unknown>,
  field: string,
  request: ResolvedPaginationRequest
): ValuePageResult<Record<string, unknown>> | null {
  if (!isPlainObject(target[field])) return null;
  return paginateObjectFieldCore(target, field, request, {}, (v, r) =>
    paginateFallbackObjectValue(v as Record<string, unknown>, r)
  );
}

function paginateFallbackValue(
  value: unknown,
  request: ResolvedPaginationRequest
): ValuePageResult<unknown> | null {
  if (typeof value === 'string') {
    return paginateStringValue(value, request);
  }

  if (isPlainObject(value)) {
    return paginateFallbackObjectValue(value, request);
  }

  return null;
}

function paginateFallbackObjectValue(
  target: Record<string, unknown>,
  request: ResolvedPaginationRequest
): ValuePageResult<Record<string, unknown>> {
  const arrayConfigs: CollectionConfig[] = [];

  for (const [field, value] of Object.entries(target)) {
    if (FALLBACK_EXCLUDED_FIELDS.has(field)) {
      continue;
    }
    if (Array.isArray(value) && value.length > 0) {
      arrayConfigs.push({
        field,
        kind: 'array',
        itemPaginator: paginateFallbackValue,
      });
    }
  }

  const arrayPage = paginateConfiguredObjectValue(
    target,
    request,
    arrayConfigs
  );
  if (arrayPage.paginated) {
    return arrayPage;
  }

  for (const [field, value] of Object.entries(target)) {
    if (FALLBACK_EXCLUDED_FIELDS.has(field) || typeof value !== 'string') {
      continue;
    }
    const fieldPage = paginateObjectStringField(target, field, request);
    if (fieldPage?.paginated) {
      return fieldPage;
    }
  }

  for (const [field, value] of Object.entries(target)) {
    if (FALLBACK_EXCLUDED_FIELDS.has(field) || !isPlainObject(value)) {
      continue;
    }
    const fieldPage = paginateNestedObjectField(target, field, request);
    if (fieldPage?.paginated) {
      return fieldPage;
    }
  }

  const totalChars = serialize(target).length;
  return {
    value: target,
    actualOffset: 0,
    pageEnd: totalChars,
    totalChars,
    paginated: false,
  };
}

function paginateGitHubSearchCodeFile(
  value: unknown,
  request: ResolvedPaginationRequest
): ValuePageResult<unknown> | null {
  if (!isPlainObject(value)) {
    return null;
  }

  return paginateConfiguredObjectValue(value, request, [
    {
      field: 'text_matches',
      kind: 'array',
      itemPaginator: (item, nestedRequest) => {
        if (typeof item === 'string') {
          return paginateStringValue(item, nestedRequest);
        }
        if (isPlainObject(item)) {
          return paginateObjectStringField(
            item as Record<string, unknown>,
            'value',
            nestedRequest
          );
        }
        return null;
      },
    },
  ]);
}

/**
 * Escape valve for an oversized single PR. A `fullContent` PR carries a
 * `fileChanges[]` array whose `patch` strings dominate the payload; without
 * sub-slicing, one PR's diff (~12KB seen live) is emitted whole on page 1,
 * blowing past the char budget. This paginates the fileChanges array and, when
 * a single patch still overflows, slices that patch string — so a page stays
 * near the budget and the rest is reachable via the cursor (lossless).
 */
function paginatePullRequest(
  value: unknown,
  request: ResolvedPaginationRequest
): ValuePageResult<unknown> | null {
  if (!isPlainObject(value)) {
    return null;
  }

  // Only engage the escape valve for a genuinely oversized PR — one whose own
  // serialized size exceeds a WHOLE page. `request.length` here is the leftover
  // budget on the current page, not a full page, so comparing against it would
  // sub-slice a normal PR that merely crossed the boundary (leaving a sub-char
  // remainder + a spurious extra page). A PR that fits in a full page is
  // cleaner deferred WHOLE to the next page by the array paginator.
  if (serialize(value).length <= getDefaultCharLength()) {
    return null;
  }

  return paginateConfiguredObjectValue(value, request, [
    {
      field: 'fileChanges',
      kind: 'array',
      itemPaginator: (item, nestedRequest) =>
        isPlainObject(item)
          ? paginateObjectStringField(
              item as Record<string, unknown>,
              'patch',
              nestedRequest
            )
          : null,
    },
  ]);
}

function paginateLocalSearchMatch(
  value: unknown,
  request: ResolvedPaginationRequest
): ValuePageResult<unknown> | null {
  if (!isPlainObject(value)) {
    return null;
  }

  return paginateObjectStringField(value, 'value', request);
}

function paginateLocalSearchFile(
  value: unknown,
  request: ResolvedPaginationRequest
): ValuePageResult<unknown> | null {
  if (!isPlainObject(value)) {
    return null;
  }

  return paginateConfiguredObjectValue(value, request, [
    {
      field: 'matches',
      kind: 'array',
      itemPaginator: paginateLocalSearchMatch,
    },
  ]);
}

function paginateLspLocation(
  value: unknown,
  request: ResolvedPaginationRequest
): ValuePageResult<unknown> | null {
  if (!isPlainObject(value)) {
    return null;
  }

  return paginateObjectStringField(value, 'content', request);
}

function paginateCallHierarchyNode(
  value: unknown,
  request: ResolvedPaginationRequest
): ValuePageResult<unknown> | null {
  if (!isPlainObject(value)) {
    return null;
  }
  // A call node nests the resolved item under `from` (incoming) or `to`
  // (outgoing); the heavy field is that nested node's `content` snippet. Slice
  // THAT field specifically (not via the generic fallback, which would waste
  // the budget slicing the first short string it finds). This is what lets us
  // drop the per-node content pre-clip and stay lossless.
  const nestedKey = isPlainObject(value.from)
    ? 'from'
    : isPlainObject(value.to)
      ? 'to'
      : null;
  if (nestedKey) {
    return paginateObjectFieldCore(
      value,
      nestedKey,
      request,
      {},
      (inner, req) => {
        const node = inner as Record<string, unknown>;
        const sliced = paginateObjectStringField(node, 'content', req);
        if (sliced) return sliced;
        const total = serialize(node).length;
        return {
          value: node,
          actualOffset: 0,
          pageEnd: total,
          totalChars: total,
          paginated: false,
        };
      }
    );
  }
  return paginateObjectStringField(value, 'content', request);
}

function pageToolDataValue(
  toolName: string,
  data: Record<string, unknown>,
  request: ResolvedPaginationRequest
): ValuePageResult<Record<string, unknown>> {
  let page: ValuePageResult<Record<string, unknown>>;

  switch (toolName) {
    case TOOL_NAMES.GITHUB_SEARCH_CODE:
      page = paginateConfiguredObjectValue(data, request, [
        {
          field: 'files',
          kind: 'array',
          itemPaginator: paginateGitHubSearchCodeFile,
        },
      ]);
      break;
    case TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES:
      // A repository is the ATOMIC pagination unit — NO itemPaginator, so the
      // segment engine includes or defers each repo WHOLE and never slices a
      // repo's internal arrays. (The old paginateGitHubRepository sub-sliced
      // `topics`, truncating it mid-element — e.g. `["dx","f"]` — so the same
      // repo rendered different topics depending on where the char window
      // landed. Items are bounded; a lone oversized repo is emitted whole for
      // forward progress, matching the "char-window only an oversized single
      // item" model that PR fileChanges still uses via paginatePullRequest.)
      page = paginateConfiguredObjectValue(data, request, [
        { field: 'repositories', kind: 'array' },
      ]);
      break;
    case TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE:
      // A directory node is the ATOMIC unit — NO itemPaginator, so the char
      // backstop emits or defers a whole node and never slices its files[] /
      // folders[] mid-list. The entry cursor (entriesPerPage→itemsPerPage)
      // already bounds page size, so nodes stay small; this matches the
      // item-atomic model used by repositories / packages / local structure.
      page = paginateConfiguredObjectValue(data, request, [
        { field: 'structure', kind: 'record' },
      ]);
      break;
    case TOOL_NAMES.PACKAGE_SEARCH:
      // A package is the ATOMIC unit — see the repositories case. No
      // itemPaginator, so keywords/engines/dependencies are never sliced
      // mid-collection; a package is included or deferred whole.
      page = paginateConfiguredObjectValue(data, request, [
        { field: 'packages', kind: 'array' },
      ]);
      break;
    case TOOL_NAMES.GITHUB_CLONE_REPO:
      page = paginateConfiguredObjectValue(data, request, [
        {
          field: 'hints',
          kind: 'array',
          itemPaginator: (item, nestedRequest) =>
            typeof item === 'string'
              ? paginateStringValue(item, nestedRequest)
              : null,
        },
      ]);
      break;
    case TOOL_NAMES.LOCAL_RIPGREP:
      page = paginateConfiguredObjectValue(data, request, [
        {
          field: 'files',
          kind: 'array',
          itemPaginator: paginateLocalSearchFile,
        },
      ]);
      break;
    case TOOL_NAMES.LOCAL_VIEW_STRUCTURE:
      page = paginateConfiguredObjectValue(data, request, [
        {
          field: 'entries',
          kind: 'array',
        },
      ]);
      break;
    case TOOL_NAMES.LSP_FIND_REFERENCES:
    case TOOL_NAMES.LSP_GOTO_DEFINITION:
      page = paginateConfiguredObjectValue(data, request, [
        {
          field: 'locations',
          kind: 'array',
          itemPaginator: paginateLspLocation,
        },
      ]);
      break;
    case TOOL_NAMES.LSP_CALL_HIERARCHY:
      page = paginateConfiguredObjectValue(data, request, [
        {
          field: 'incomingCalls',
          kind: 'array',
          itemPaginator: paginateCallHierarchyNode,
        },
        {
          field: 'outgoingCalls',
          kind: 'array',
          itemPaginator: paginateCallHierarchyNode,
        },
        // `calls` is emitted when the LSP server is unavailable and the tool
        // falls back to pattern-matching (lspMode='fallback'). Include it here
        // so the dedicated node paginator handles it instead of falling through
        // to the generic array paginator, which produces less precise chunking.
        {
          field: 'calls',
          kind: 'array',
          itemPaginator: paginateCallHierarchyNode,
        },
      ]);
      break;
    case TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS:
      // The unified engine owns char-pagination: it slices the pull_requests
      // array and — via paginatePullRequest — sub-slices an oversized single PR
      // (its fileChanges[].patch under fullContent) so one giant diff can't blow
      // the page budget. Lossless: the rest is reachable through the cursor.
      page = paginateConfiguredObjectValue(data, request, [
        {
          field: 'pull_requests',
          kind: 'array',
          itemPaginator: paginatePullRequest,
        },
      ]);
      break;
    case TOOL_NAMES.LOCAL_FIND_FILES: {
      // localFindFiles runs its own char-pagination inside findFiles.ts and
      // stores the result as `charPagination` (upstream type name). Promote
      // it to the canonical `outputPagination` key so the bulk engine can
      // surface the cursor uniformly. No re-slicing: the tool already applied
      // the window; here we only alias the field name.
      const promotedData = promoteCharPagination(data);
      const len = serialize(promotedData).length;
      // When the file-level pagination has more pages remaining, signal it to
      // the bulk engine by making totalChars > pageEnd. This causes
      // createOutputPagination to return hasMore=true in responsePagination,
      // preventing agents from thinking all data was delivered when 13 more
      // file pages remain (the file cursor is advanced via page=N, not charOffset).
      const filePagination = data.pagination as
        | { hasMore?: boolean }
        | undefined;
      return {
        value: promotedData,
        actualOffset: 0,
        pageEnd: len,
        totalChars: filePagination?.hasMore ? len + 1 : len,
        paginated: false,
      };
    }
    case TOOL_NAMES.GITHUB_FETCH_CONTENT:
    case TOOL_NAMES.LOCAL_FETCH_CONTENT:
      return {
        value: data,
        actualOffset: 0,
        pageEnd: serialize(data).length,
        totalChars: serialize(data).length,
        paginated: false,
      };
    default:
      page = {
        value: data,
        actualOffset: 0,
        pageEnd: serialize(data).length,
        totalChars: serialize(data).length,
        paginated: false,
      };
      break;
  }

  return page.paginated ? page : paginateFallbackObjectValue(data, request);
}

function paginateFlatQueryResult(
  value: unknown,
  request: ResolvedPaginationRequest,
  toolName: string
): ValuePageResult<unknown> | null {
  if (!isPlainObject(value) || !isPlainObject(value.data)) {
    return null;
  }

  // outputPagination is only valid on the success branch. Success is
  // signaled by ABSENT status — emitted explicitly only for 'empty' /
  // 'error', where outputPagination is rejected by the strict schemas.
  if (value.status !== undefined) {
    return null;
  }

  const baseValue = {
    ...value,
    data: {},
  };
  const wrapperChars = Math.max(serialize(baseValue).length - 2, 0);
  const dataPage = pageToolDataValue(toolName, value.data, {
    offset: request.offset <= wrapperChars ? 0 : request.offset - wrapperChars,
    length:
      request.offset <= wrapperChars
        ? Math.max(1, request.length - wrapperChars)
        : request.length,
    explicit: true,
  });
  const totalChars = wrapperChars + dataPage.totalChars;

  if (
    !dataPage.paginated &&
    (!request.explicit || totalChars <= request.length)
  ) {
    return {
      value,
      actualOffset: 0,
      pageEnd: totalChars,
      totalChars,
      paginated: false,
    };
  }

  if (request.offset >= totalChars) {
    return {
      value: baseValue,
      actualOffset: request.offset,
      pageEnd: request.offset,
      totalChars,
      paginated: true,
    };
  }

  const shouldExposeQueryOutputPagination =
    toolName !== TOOL_NAMES.LSP_FIND_REFERENCES;
  const dataPagination = createOutputPagination(
    dataPage.actualOffset,
    Math.max(0, dataPage.pageEnd - dataPage.actualOffset),
    dataPage.totalChars,
    request.length
  );
  const dataValue =
    dataPage.paginated && isPlainObject(dataPage.value)
      ? shouldExposeQueryOutputPagination
        ? withPaginationHints(
            {
              ...dataPage.value,
              outputPagination: dataPagination,
              ...(toolName === TOOL_NAMES.LOCAL_FIND_FILES && {
                charPagination: dataPagination,
              }),
            },
            dataPagination,
            'output',
            {
              autoPaginated: false,
              requestedLength: request.length,
            }
          )
        : dataPage.value
      : dataPage.value;

  return {
    value: {
      ...value,
      data: dataValue,
    },
    actualOffset:
      request.offset <= wrapperChars ? 0 : wrapperChars + dataPage.actualOffset,
    pageEnd: wrapperChars + dataPage.pageEnd,
    totalChars,
    paginated: true,
  };
}

export function applyQueryOutputPagination(
  queryResult: FlatQueryResult,
  originalQuery: Record<string, unknown>,
  toolName: string
): FlatQueryResult {
  if (!isPlainObject(queryResult.data)) {
    return queryResult;
  }

  // outputPagination is only valid on the success branch. Both
  // ErrorDataSchema and EmptyDataSchema are strict and do not declare
  // outputPagination, so injecting it there would trigger MCP output
  // validation failures. Success is signaled by ABSENT status.
  if (queryResult.status !== undefined) {
    return queryResult;
  }

  if (toolName === TOOL_NAMES.LSP_FIND_REFERENCES) {
    return queryResult;
  }

  const request = resolveRequest({
    offset:
      typeof originalQuery.charOffset === 'number'
        ? originalQuery.charOffset
        : undefined,
    length:
      typeof originalQuery.charLength === 'number'
        ? originalQuery.charLength
        : undefined,
  });

  // Per-query char-pagination engages ONLY when the caller explicitly navigates
  // a single query via charOffset/charLength. Auto-capping the whole response
  // is owned solely by applyBulkResponsePagination, so the agent gets ONE
  // coherent cursor (responseCharOffset) instead of two breadcrumbs reporting
  // different char totals (the per-query pre-slice total vs the bulk total).
  if (!request.explicit) {
    // localFindFiles handles its own char-pagination inside findFiles.ts; promote
    // charPagination → outputPagination so agents see a uniform cursor field.
    // `promoteCharPagination` returns the same object reference when there is
    // nothing to promote, so we preserve reference equality on the no-op path.
    if (toolName === TOOL_NAMES.LOCAL_FIND_FILES) {
      const promoted = promoteCharPagination(queryResult.data);
      if (promoted === queryResult.data) return queryResult;
      return { ...queryResult, data: promoted };
    }
    return queryResult;
  }

  const page = pageToolDataValue(toolName, queryResult.data, request);

  if (!page.paginated) {
    // localFindFiles: same charPagination promotion as the non-explicit path above.
    if (toolName === TOOL_NAMES.LOCAL_FIND_FILES) {
      const promoted = promoteCharPagination(
        page.value as Record<string, unknown>
      );
      if (promoted === queryResult.data) return queryResult;
      return { ...queryResult, data: promoted };
    }
    return queryResult;
  }

  const pagination = createOutputPagination(
    page.actualOffset,
    Math.max(0, page.pageEnd - page.actualOffset),
    page.totalChars,
    request.length
  );
  const nextData = withPaginationHints(
    {
      ...page.value,
      outputPagination: pagination,
      // localFindFiles: also write `charPagination` so the upstream type
      // contract is satisfied when the bulk engine re-slices the tool output.
      ...(toolName === TOOL_NAMES.LOCAL_FIND_FILES && {
        charPagination: pagination,
      }),
    },
    pagination,
    'output',
    {
      autoPaginated: !request.explicit,
      requestedLength: request.length,
    }
  );

  return {
    ...queryResult,
    data: nextData,
  };
}

export function applyBulkResponsePagination(
  response: BulkToolResponse,
  request: PaginationRequest,
  toolName: string
): BulkToolResponse {
  // Reserve one base window per query so a large first query doesn't starve its
  // siblings off page 1 (#3). Only affects the auto-pagination default — an
  // explicit responseCharOffset/Length still wins via resolveRequest.
  const resolvedRequest = resolveRequest(
    request,
    getBulkDefaultCharLength(response.results.length)
  );

  // Single coherent cursor: when the caller drove PER-QUERY pagination (every
  // result already carries an outputPagination cursor from an explicit
  // charOffset/charLength) and did NOT request bulk pagination, the per-query
  // slices already bound the response. Re-paginating here would emit a SECOND
  // breadcrumb with a different char total — the contradictory-cursor smell.
  // So skip the bulk pass and let the per-query cursor stand alone.
  if (
    !resolvedRequest.explicit &&
    response.results.length > 0 &&
    response.results.every(
      r =>
        isPlainObject(r?.data) &&
        (r.data as Record<string, unknown>).outputPagination !== undefined
    )
  ) {
    return response;
  }

  const page = paginateConfiguredObjectValue(
    { results: response.results },
    resolvedRequest,
    [
      {
        field: 'results',
        kind: 'array',
        itemPaginator: (value, nestedRequest) =>
          paginateFlatQueryResult(value, nestedRequest, toolName),
      },
    ]
  );

  if (!page.paginated) {
    return response;
  }

  return {
    results: Array.isArray(page.value.results)
      ? (page.value.results as FlatQueryResult[])
      : [],
    responsePagination: createOutputPagination(
      page.actualOffset,
      Math.max(0, page.pageEnd - page.actualOffset),
      page.totalChars,
      resolvedRequest.length
    ),
  };
}
