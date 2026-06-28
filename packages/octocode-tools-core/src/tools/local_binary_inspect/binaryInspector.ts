import { promises as fs } from 'fs';
import { join, basename, dirname, resolve, sep } from 'node:path';
import { securityRegistry } from '@octocodeai/octocode-engine/registry';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { paths } from '../../shared/paths.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { contextUtils } from '../../utils/contextUtils.js';
import { applyPagination } from '../../utils/pagination/core.js';
import { getOutputCharLimit } from '../../utils/pagination/charLimit.js';
import type { BinaryInspectQuery } from './scheme.js';
import {
  listArchiveEntries,
  extractArchiveEntry,
  extractArchiveToDir,
} from './archiveOps.js';
import { decompressFile } from './decompressOps.js';
import { inspectBinaryFile, extractStrings } from './binaryOps.js';

const TOOL_NAME = TOOL_NAMES.LOCAL_BINARY_INSPECT;

// strings mode always writes the full blob to `localPath` for grepping, so the
// inline window is just a preview — cap it well below the global 20k default to
// avoid a heavy context hit. Lossless: charOffset/charLength still page the
// window and scanOffset/nextScanOffset advance across the file.
const STRINGS_INLINE_PREVIEW_LIMIT = 4_000;

function timestampForPath(date = new Date()): string {
  return date.toISOString().replace(/[:.]/g, '-');
}

function unpackDestination(path: string): string {
  return join(paths.unzip, `${basename(path)}-${timestampForPath()}`);
}

function derivedTextRoot(path: string, mode: string): string {
  return join(paths.binary, `${basename(path)}-${mode}-${timestampForPath()}`);
}

function safeRelativeOutputPath(name: string): string {
  const normalized = name
    .replace(/\\/g, '/')
    .split('/')
    .filter(segment => segment && segment !== '.' && segment !== '..')
    .join('/');
  return normalized || 'content.txt';
}

async function writeDerivedTextFile(
  sourcePath: string,
  mode: string,
  suggestedName: string,
  content: string
): Promise<string> {
  const root = resolve(derivedTextRoot(sourcePath, mode));
  const outputPath = resolve(join(root, safeRelativeOutputPath(suggestedName)));
  if (!outputPath.startsWith(root + sep) && outputPath !== root) {
    throw new Error('Derived binary output path escaped its tmp directory.');
  }
  await fs.mkdir(dirname(outputPath), { recursive: true });
  await fs.writeFile(outputPath, content, 'utf-8');
  return outputPath;
}

// The container-lane backends shell out to external CLIs that are not in the
// base security allowlist (rg/ls/find/grep/git). Register them here so the tool
// can execute. Idempotent. The format lane (inspect/strings) is fully native
// (octocode-engine) and needs no allowlisted command; the binutils commands
// `xxd`/`strings` were removed with the old identify/strings shell-outs. `file`
// stays — decompress still uses `file --mime-type` for format auto-detection.
const BINARY_BACKEND_COMMANDS = [
  'file',
  'unzip',
  'tar',
  'bsdtar',
  '7z',
  '7zz',
  'aa',
  'zcat',
  'gunzip',
  'bzcat',
  'xzcat',
  'zstdcat',
  'zstd',
  'lz4cat',
  'brotli',
  'lzfse',
];
let binaryCommandsRegistered = false;
function registerBinaryCommands(): void {
  if (binaryCommandsRegistered) return;
  try {
    securityRegistry.addAllowedCommands(BINARY_BACKEND_COMMANDS);
  } catch {
    /* ignore — validation will surface a clear error if a command is blocked */
  }
  binaryCommandsRegistered = true;
}

const DEFAULT_MAX_ENTRIES = 1000;
const DEFAULT_MIN_STRING_LENGTH = 8;

// ─── helpers ──────────────────────────────────────────────────────────────────

function filterByMatchString(
  content: string,
  matchString: string,
  contextLines: number
): string | null {
  const result = contextUtils.extractMatchingLines(content, matchString, {
    isRegex: true,
    caseSensitive: false,
    contextLines,
  });
  return result.lines.length > 0 ? result.lines.join('\n') : null;
}

interface ContentCharPagination {
  currentPage: number;
  totalPages: number;
  hasMore: boolean;
  charOffset: number;
  charLength: number;
  totalChars: number;
  nextCharOffset?: number;
}

function paginateContent(
  content: string,
  charOffset: number | undefined,
  charLength: number | undefined,
  defaultLimit: number
): { content: string; isPartial: boolean; pagination?: ContentCharPagination } {
  const limit = charLength ?? defaultLimit;
  const offset = charOffset ?? 0;
  const meta = applyPagination(content, offset, limit);

  // Surface the char cursor as structured data (the only continuation signal,
  // now that the prose `charOffset=N` hint is gone). Emitted only when the
  // content actually spans more than one page, matching localGetFileContent.
  const pagination: ContentCharPagination | undefined =
    meta.hasMore || meta.totalPages > 1
      ? {
          currentPage: meta.currentPage,
          totalPages: meta.totalPages,
          hasMore: meta.hasMore,
          charOffset: meta.charOffset,
          charLength: meta.charLength,
          totalChars: meta.totalChars,
          ...(meta.hasMore && meta.nextCharOffset !== undefined
            ? { nextCharOffset: meta.nextCharOffset }
            : {}),
        }
      : undefined;

  return {
    content: meta.paginatedContent,
    isPartial: meta.hasMore,
    pagination,
  };
}

// ─── mode handlers ────────────────────────────────────────────────────────────

function handleInspect(path: string, query: BinaryInspectQuery) {
  const result = inspectBinaryFile(path);
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

async function handleList(path: string, query: BinaryInspectQuery) {
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
      },
    }),
  };
}

async function handleExtract(path: string, query: BinaryInspectQuery) {
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
  };
}

async function handleDecompress(path: string, query: BinaryInspectQuery) {
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
  };
}

async function handleStrings(path: string, query: BinaryInspectQuery) {
  const minLength = query.minLength ?? DEFAULT_MIN_STRING_LENGTH;
  const includeOffsets = query.includeOffsets ?? false;
  const scanOffset = query.scanOffset ?? 0;
  const result = extractStrings(path, minLength, includeOffsets, scanOffset);

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
  };
}

async function handleUnpack(path: string, query: BinaryInspectQuery) {
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

// ─── main entry ───────────────────────────────────────────────────────────────

export async function inspectBinary(query: BinaryInspectQuery) {
  registerBinaryCommands();

  const validation = validateToolPath(query, TOOL_NAME);
  if (!validation.isValid) return validation.errorResult;

  const filePath = validation.sanitizedPath;

  try {
    const stat = await fs.stat(filePath);
    if (!stat.isFile()) {
      return createErrorResult(
        `Path is not a regular file: ${filePath}`,
        query
      );
    }
  } catch {
    return createErrorResult(`File not found: ${filePath}`, query);
  }

  switch (query.mode) {
    case 'inspect':
      return handleInspect(filePath, query);
    case 'list':
      return handleList(filePath, query);
    case 'extract':
      return handleExtract(filePath, query);
    case 'decompress':
      return handleDecompress(filePath, query);
    case 'strings':
      return handleStrings(filePath, query);
    case 'unpack':
      return handleUnpack(filePath, query);
    default:
      return createErrorResult(
        `Unknown mode: ${String((query as BinaryInspectQuery).mode)}`,
        query
      );
  }
}
