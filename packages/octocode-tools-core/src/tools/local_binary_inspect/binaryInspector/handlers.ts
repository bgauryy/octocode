import { promises as fs } from 'fs';
import { basename } from 'node:path';
import { createErrorResult } from '../../../utils/file/toolHelpers.js';
import { getOutputCharLimit } from '../../../utils/pagination/charLimit.js';
import type { BinaryInspectQuery } from '../scheme.js';
import {
  listArchiveEntries,
  extractArchiveEntry,
  extractArchiveToDir,
} from '../archiveOps.js';
import { decompressFile } from '../decompressOps.js';
import { inspectBinaryFile, extractStrings } from '../binaryOps.js';
import { buildNextPageContinuation } from '../../../scheme/pagination.js';
import {
  TOOL_NAME,
  unpackDestination,
  writeDerivedTextFile,
  filterByMatchString,
  paginateContent,
  attachCharContinuation,
} from './helpers.js';

// strings mode always writes the full blob to `localPath` for grepping, so the
// inline window is just a preview — cap it well below the global 20k default to
// avoid a heavy context hit. Lossless: charOffset/charLength still page the
// window and scanOffset/nextScanOffset advance across the file.
const STRINGS_INLINE_PREVIEW_LIMIT = 4_000;

const DEFAULT_MAX_ENTRIES = 1000;
const DEFAULT_MIN_STRING_LENGTH = 8;

export async function handleInspect(path: string, query: BinaryInspectQuery) {
  const result = await inspectBinaryFile(path);
  if (!result.success || !result.info) {
    return createErrorResult(result.error ?? 'inspect failed', query);
  }
  const info = result.info;
  const detailed = query.detailed ?? false;
  return {
    status: 'success' as const,
    mode: 'inspect' as const,
    path,
    format: info.format,
    description: info.description,
    magicBytes: info.magicHex,
    ...(info.arch ? { arch: info.arch } : {}),
    ...(info.bits ? { bits: info.bits } : {}),
    ...(info.endianness ? { endianness: info.endianness } : {}),
    ...(info.stripped !== undefined ? { stripped: info.stripped } : {}),
    ...(info.entry ? { entry: info.entry } : {}),
    symbolCount: info.symbolCount,
    importCount: info.importCount,
    exportCount: info.exportCount,
    ...(detailed && info.symbols.length ? { symbols: info.symbols } : {}),
    ...(detailed && info.imports.length ? { imports: info.imports } : {}),
    ...(detailed && info.exports.length ? { exports: info.exports } : {}),
    ...(detailed && info.sections.length ? { sections: info.sections } : {}),
    ...(info.libraries.length ? { libraries: info.libraries } : {}),
    ...(detailed ? { detailed: true } : {}),
    ...(info.truncated ? { truncated: true } : {}),
    ...(info.notes.length ? { warnings: info.notes } : {}),
  };
}

export async function handleList(path: string, query: BinaryInspectQuery) {
  const verbose = query.verbose ?? false;
  const result = await listArchiveEntries(path, verbose);

  if (!result.success) {
    return createErrorResult(
      result.stderr || 'All archive backends failed',
      query
    );
  }

  const all = result.entries ?? [];
  const cap = Math.min(query.maxEntries ?? DEFAULT_MAX_ENTRIES, all.length);
  const capped = all.slice(0, cap);

  const perPage = query.entriesPerPage;
  const page = query.entryPageNumber ?? 1;
  const entries = perPage
    ? capped.slice((page - 1) * perPage, page * perPage)
    : capped;

  const totalPages = perPage ? Math.ceil(capped.length / perPage) : 1;
  const hasMore = perPage ? page < totalPages : false;

  return {
    status: 'success' as const,
    mode: 'list' as const,
    path,
    backend: result.commandUsed,
    // Authoritative total (uncapped count of all archive entries); read by the
    // CLI renderer. Previously also duplicated inside pagination under the SAME
    // name with the capped count — a name collision, now removed.
    totalEntries: all.length,
    entries,
    ...(perPage && {
      pagination: {
        currentPage: page,
        totalPages,
        hasMore,
        entriesPerPage: perPage,
        ...(hasMore ? { nextPage: page + 1 } : {}),
      },
    }),
    ...(hasMore
      ? {
          next: {
            nextPage: buildNextPageContinuation(
              TOOL_NAME,
              {
                ...query,
                path,
                mode: 'list',
                entryPageNumber: page + 1,
              } as Record<string, unknown>,
              'Continue to the next page of archive entries.'
            ),
          },
        }
      : {}),
  };
}

export async function handleExtract(path: string, query: BinaryInspectQuery) {
  const archiveFile = query.archiveFile!;
  const result = await extractArchiveEntry(path, archiveFile);

  if (!result.success) {
    return createErrorResult(result.stderr || 'Extraction failed', query);
  }

  let content = result.stdout;
  if (!content) {
    return createErrorResult('Entry is empty', query);
  }
  const localPath = await writeDerivedTextFile(
    path,
    'extract',
    archiveFile,
    content
  );

  if (query.matchString) {
    const filtered = filterByMatchString(
      content,
      query.matchString,
      query.matchStringContextLines ?? 3
    );
    if (!filtered) {
      return createErrorResult(
        `No lines match "${query.matchString}" in the extracted entry`,
        query
      );
    }
    content = filtered;
  }

  const defaultLimit = getOutputCharLimit();
  const paginated = paginateContent(
    content,
    query.charOffset,
    query.charLength,
    defaultLimit
  );

  return {
    status: 'success' as const,
    mode: 'extract' as const,
    path,
    archiveFile,
    backend: result.commandUsed,
    localPath,
    content: paginated.content,
    isPartial: paginated.isPartial,
    ...(paginated.pagination ? { pagination: paginated.pagination } : {}),
    ...attachCharContinuation(query, path, 'extract', paginated, {
      archiveFile,
    }),
  };
}

export async function handleDecompress(
  path: string,
  query: BinaryInspectQuery
) {
  const result = await decompressFile(path, query.format ?? 'auto');

  if (!result.success) {
    return createErrorResult(result.error ?? 'Decompression failed', query);
  }

  let content = result.content ?? '';
  if (!content) {
    return createErrorResult('Decompressed file is empty', query);
  }
  const localPath = await writeDerivedTextFile(
    path,
    'decompress',
    `${basename(path)}.decompressed.txt`,
    content
  );

  if (query.matchString) {
    const filtered = filterByMatchString(
      content,
      query.matchString,
      query.matchStringContextLines ?? 3
    );
    if (!filtered) {
      return createErrorResult(
        `No lines match "${query.matchString}" in the decompressed content`,
        query
      );
    }
    content = filtered;
  }

  const defaultLimit = getOutputCharLimit();
  const paginated = paginateContent(
    content,
    query.charOffset,
    query.charLength,
    defaultLimit
  );

  return {
    status: 'success' as const,
    mode: 'decompress' as const,
    path,
    format: result.format,
    backend: result.backend,
    localPath,
    content: paginated.content,
    isPartial: paginated.isPartial,
    ...(paginated.pagination ? { pagination: paginated.pagination } : {}),
    ...attachCharContinuation(query, path, 'decompress', paginated),
  };
}

export async function handleStrings(path: string, query: BinaryInspectQuery) {
  const minLength = query.minLength ?? DEFAULT_MIN_STRING_LENGTH;
  const includeOffsets = query.includeOffsets ?? false;
  const scanOffset = query.scanOffset ?? 0;
  const result = await extractStrings(
    path,
    minLength,
    includeOffsets,
    scanOffset
  );

  if (!result.success) {
    return createErrorResult(
      result.error ?? 'strings extraction failed',
      query
    );
  }

  // Two complementary, lossless cursors:
  //  • charOffset/charLength — pages the strings *within* the current scan
  //    window (the joined blob), exactly like decompress/extract.
  //  • scanOffset/nextScanOffset — advances the scan *window* across the whole
  //    file. The window is rewound to a safe break, so no string is split and
  //    nothing past a fixed cap is discarded. Exhaust charOffset first, then
  //    follow nextScanOffset to keep scanning.
  let content = (result.strings ?? []).join('\n');
  const localPath = content
    ? await writeDerivedTextFile(
        path,
        'strings',
        `${basename(path)}.strings.txt`,
        content
      )
    : undefined;
  if (query.matchString) {
    const filtered = filterByMatchString(
      content,
      query.matchString,
      query.matchStringContextLines ?? 3
    );
    if (!filtered) {
      return createErrorResult(
        `No lines match "${query.matchString}" in extracted strings`,
        query
      );
    }
    content = filtered;
  }
  // When the full strings are on disk and the caller hasn't asked for an
  // explicit window or a matchString filter, preview a small slice inline and
  // point at localPath for grep; otherwise honor the global window.
  const usePreview =
    Boolean(localPath) &&
    query.charOffset === undefined &&
    query.charLength === undefined &&
    !query.matchString;
  const defaultLimit = usePreview
    ? Math.min(STRINGS_INLINE_PREVIEW_LIMIT, getOutputCharLimit())
    : getOutputCharLimit();
  const paginated = paginateContent(
    content,
    query.charOffset,
    query.charLength,
    defaultLimit
  );

  return {
    status: 'success' as const,
    mode: 'strings' as const,
    path,
    content: paginated.content,
    ...(localPath ? { localPath } : {}),
    totalFound: result.totalFound ?? 0,
    isPartial: paginated.isPartial,
    ...(paginated.pagination ? { pagination: paginated.pagination } : {}),
    scanOffset,
    ...(result.nextScanOffset !== undefined
      ? { nextScanOffset: result.nextScanOffset }
      : {}),
    ...attachCharContinuation(query, path, 'strings', paginated, {
      ...(result.nextScanOffset !== undefined
        ? { nextScanOffset: result.nextScanOffset }
        : {}),
    }),
  };
}

export async function handleUnpack(path: string, query: BinaryInspectQuery) {
  try {
    await fs.stat(path);
  } catch {
    return createErrorResult(`File not found: ${path}`, query);
  }

  const destDir = unpackDestination(path);
  await fs.mkdir(destDir, { recursive: true });

  const result = await extractArchiveToDir(path, destDir);
  if (!result.success) {
    return createErrorResult(
      `Unpack failed: ${result.stderr || 'no backend could extract this archive'}`,
      query
    );
  }

  let topLevelEntries = 0;
  try {
    topLevelEntries = (await fs.readdir(destDir)).length;
  } catch {
    /* ignore */
  }

  return {
    status: 'success' as const,
    mode: 'unpack' as const,
    path,
    localPath: destDir,
    cached: false,
    topLevelEntries,
  };
}
