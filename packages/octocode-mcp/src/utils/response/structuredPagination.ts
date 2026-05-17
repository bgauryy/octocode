import { getConfigSync } from 'octocode-shared';
import { TOOL_NAMES } from '../../tools/toolMetadata/proxies.js';
import type {
  BulkToolResponse,
  FlatQueryResult,
  PaginationInfo,
} from '../../types.js';

const DEFAULT_OUTPUT_CHAR_LENGTH = 8000;
const FALLBACK_EXCLUDED_FIELDS = new Set([
  'hints',
  'warnings',
  'pagination',
  'outputPagination',
  'charPagination',
  'responsePagination',
]);

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

function readConfiguredDefaultCharLength(): number {
  const config = getConfigSync() as {
    output?: {
      pagination?: {
        defaultCharLength?: number;
      };
    };
  };

  return (
    config.output?.pagination?.defaultCharLength ?? DEFAULT_OUTPUT_CHAR_LENGTH
  );
}

function getDefaultCharLength(): number {
  try {
    return readConfiguredDefaultCharLength();
  } catch {
    return DEFAULT_OUTPUT_CHAR_LENGTH;
  }
}

function resolveRequest(request: PaginationRequest): ResolvedPaginationRequest {
  return {
    offset: request.offset ?? 0,
    length: Math.max(request.length ?? getDefaultCharLength(), 1),
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
  const totalPages = Math.max(1, Math.ceil(safeTotalChars / safePageSize));
  const maxLogicalOffset =
    safeTotalChars === 0 ? 0 : Math.max(safeTotalChars - 1, 0);
  const pageOffset = Math.min(Math.max(charOffset, 0), maxLogicalOffset);

  return {
    currentPage:
      safeTotalChars === 0
        ? 1
        : Math.min(totalPages, Math.floor(pageOffset / safePageSize) + 1),
    totalPages,
    hasMore: charOffset + charLength < safeTotalChars,
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

function paginateGitHubRepository(
  value: unknown,
  request: ResolvedPaginationRequest
): ValuePageResult<unknown> | null {
  if (!isPlainObject(value)) {
    return null;
  }

  return paginateConfiguredObjectValue(value, request, [
    {
      field: 'topics',
      kind: 'array',
      itemPaginator: (item, nestedRequest) =>
        typeof item === 'string'
          ? paginateStringValue(item, nestedRequest)
          : null,
    },
  ]);
}

function paginatePackageEntry(
  value: unknown,
  request: ResolvedPaginationRequest
): ValuePageResult<unknown> | null {
  if (!isPlainObject(value)) {
    return null;
  }

  return paginateConfiguredObjectValue(value, request, [
    {
      field: 'keywords',
      kind: 'array',
      itemPaginator: (item, nestedRequest) =>
        typeof item === 'string'
          ? paginateStringValue(item, nestedRequest)
          : null,
    },
    {
      field: 'engines',
      kind: 'record',
    },
    {
      field: 'dependencies',
      kind: 'record',
    },
    {
      field: 'peerDependencies',
      kind: 'record',
    },
  ]);
}

function paginateStructureEntry(
  value: unknown,
  request: ResolvedPaginationRequest
): ValuePageResult<unknown> | null {
  if (!isPlainObject(value)) {
    return null;
  }

  return paginateConfiguredObjectValue(value, request, [
    {
      field: 'files',
      kind: 'array',
      itemPaginator: (item, nestedRequest) =>
        typeof item === 'string'
          ? paginateStringValue(item, nestedRequest)
          : null,
    },
    {
      field: 'folders',
      kind: 'array',
      itemPaginator: (item, nestedRequest) =>
        typeof item === 'string'
          ? paginateStringValue(item, nestedRequest)
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
      page = paginateConfiguredObjectValue(data, request, [
        {
          field: 'repositories',
          kind: 'array',
          itemPaginator: paginateGitHubRepository,
        },
      ]);
      break;
    case TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE:
      page = paginateConfiguredObjectValue(data, request, [
        {
          field: 'structure',
          kind: 'record',
          itemPaginator: paginateStructureEntry,
        },
      ]);
      break;
    case TOOL_NAMES.PACKAGE_SEARCH:
      page = paginateConfiguredObjectValue(data, request, [
        {
          field: 'packages',
          kind: 'array',
          itemPaginator: paginatePackageEntry,
        },
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
          itemPaginator: paginateFallbackValue,
        },
        {
          field: 'outgoingCalls',
          kind: 'array',
          itemPaginator: paginateFallbackValue,
        },
      ]);
      break;
    case TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS:
      if (data.outputPagination) {
        return {
          value: data,
          actualOffset: 0,
          pageEnd: serialize(data).length,
          totalChars: serialize(data).length,
          paginated: false,
        };
      }
      page = paginateConfiguredObjectValue(data, request, [
        {
          field: 'pull_requests',
          kind: 'array',
        },
      ]);
      break;
    case TOOL_NAMES.LOCAL_FIND_FILES:
      return {
        value:
          data.charPagination && !data.outputPagination
            ? { ...data, outputPagination: data.charPagination }
            : data,
        actualOffset: 0,
        pageEnd: serialize(data).length,
        totalChars: serialize(data).length,
        paginated: false,
      };
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

  // Same guard as applyQueryOutputPagination: outputPagination is only valid
  // on the hasResults branch of each tool's discriminated output schema.
  if (value.status !== 'hasResults') {
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

  // Output pagination metadata is only allowed inside the 'hasResults' branch
  // of each tool's discriminated output schema. Both ErrorDataSchema and
  // EmptyDataSchema are strict and do not declare outputPagination, so
  // injecting it there would trigger MCP output validation failures
  // (unrecognized_keys: ["outputPagination"]).
  if (queryResult.status !== 'hasResults') {
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
  const page = pageToolDataValue(toolName, queryResult.data, request);

  if (!page.paginated) {
    if (
      toolName === TOOL_NAMES.LOCAL_FIND_FILES &&
      queryResult.data.charPagination &&
      !queryResult.data.outputPagination
    ) {
      return {
        ...queryResult,
        data: {
          ...queryResult.data,
          outputPagination: queryResult.data.charPagination,
        },
      };
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
  const resolvedRequest = resolveRequest(request);
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
