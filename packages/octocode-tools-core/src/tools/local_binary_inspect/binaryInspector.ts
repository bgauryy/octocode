import { promises as fs } from 'fs';
import { securityRegistry } from 'octocode-security/registry';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { applyPagination } from '../../utils/pagination/core.js';
import { getOutputCharLimit } from '../../utils/pagination/charLimit.js';
import type { BinaryInspectQuery } from './scheme.js';
import { listArchiveEntries, extractArchiveEntry } from './archiveOps.js';
import { decompressFile } from './decompressOps.js';
import { identifyFile, extractStrings } from './binaryOps.js';

const TOOL_NAME = TOOL_NAMES.LOCAL_BINARY_INSPECT;

// The binary backends shell out to external CLIs that are not in the base
// security allowlist (rg/ls/find/grep/git). Register them here so the tool
// can execute. Idempotent.
const BINARY_BACKEND_COMMANDS = [
  'file',
  'xxd',
  'strings',
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
const DEFAULT_STRING_LIMIT = 200;

// ─── helpers ──────────────────────────────────────────────────────────────────

function filterByMatchString(
  content: string,
  matchString: string,
  contextLines: number
): string | null {
  const lines = content.split('\n');
  const pattern = new RegExp(matchString, 'i');
  const included = new Set<number>();

  for (let i = 0; i < lines.length; i++) {
    if (pattern.test(lines[i] ?? '')) {
      for (
        let c = Math.max(0, i - contextLines);
        c <= Math.min(lines.length - 1, i + contextLines);
        c++
      ) {
        included.add(c);
      }
    }
  }

  const result = Array.from(included)
    .sort((a, b) => a - b)
    .map(idx => lines[idx] ?? '');

  return result.length ? result.join('\n') : null;
}

function paginateContent(
  content: string,
  charOffset: number | undefined,
  charLength: number | undefined,
  defaultLimit: number
): { content: string; isPartial: boolean; nextCharOffset?: number } {
  const limit = charLength ?? defaultLimit;
  const offset = charOffset ?? 0;
  const meta = applyPagination(content, offset, limit);

  return {
    content: meta.paginatedContent,
    isPartial: meta.hasMore,
    nextCharOffset: meta.hasMore ? meta.nextCharOffset : undefined,
  };
}

// ─── mode handlers ────────────────────────────────────────────────────────────

async function handleIdentify(path: string, query: BinaryInspectQuery) {
  const result = await identifyFile(path);
  if (!result.success) {
    return createErrorResult(result.error ?? 'identify failed', query);
  }
  return {
    status: 'success' as const,
    mode: 'identify' as const,
    path,
    fileType: result.fileType,
    magicBytes: result.magicBytes,
  };
}

async function handleList(path: string, query: BinaryInspectQuery) {
  const verbose = query.verbose ?? false;
  const result = await listArchiveEntries(path, verbose);

  if (!result.success) {
    const missingHint = result.missingCommands?.length
      ? [`Install a missing backend: ${result.missingCommands.join(', ')}`]
      : [];
    return createErrorResult(
      result.stderr || 'All archive backends failed',
      query,
      {
        customHints: [
          ...missingHint,
          'Run mode="identify" first to confirm this is an archive.',
        ],
      }
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
    totalEntries: all.length,
    entries,
    ...(perPage && {
      pagination: {
        currentPage: page,
        totalPages,
        hasMore,
        entriesPerPage: perPage,
        totalEntries: capped.length,
      },
    }),
  };
}

async function handleExtract(path: string, query: BinaryInspectQuery) {
  const archiveFile = query.archiveFile!;
  const result = await extractArchiveEntry(path, archiveFile);

  if (!result.success) {
    const missingHint = result.missingCommands?.length
      ? [`Install a missing backend: ${result.missingCommands.join(', ')}`]
      : [];
    return createErrorResult(result.stderr || 'Extraction failed', query, {
      customHints: [
        ...missingHint,
        'Run mode="list" first — entry names are case-sensitive.',
      ],
    });
  }

  let content = result.stdout;
  if (!content) {
    return createErrorResult('Entry is empty', query);
  }

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
    content: paginated.content,
    contentLength: content.length,
    isPartial: paginated.isPartial,
    ...(paginated.nextCharOffset !== undefined && {
      hints: [`charOffset=${paginated.nextCharOffset}`],
    }),
  };
}

async function handleDecompress(path: string, query: BinaryInspectQuery) {
  const result = await decompressFile(path, query.format ?? 'auto');

  if (!result.success) {
    return createErrorResult(result.error ?? 'Decompression failed', query, {
      customHints: [
        'For multi-entry archives (.tar.gz, .zip etc.) use mode="list" or mode="extract".',
        'Set format explicitly: gzip|bzip2|xz|lzma|zstd|lz4|brotli|lzfse',
      ],
    });
  }

  let content = result.content ?? '';
  if (!content) {
    return createErrorResult('Decompressed file is empty', query);
  }

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
    content: paginated.content,
    contentLength: content.length,
    isPartial: paginated.isPartial,
    ...(paginated.nextCharOffset !== undefined && {
      hints: [`charOffset=${paginated.nextCharOffset}`],
    }),
  };
}

async function handleStrings(path: string, query: BinaryInspectQuery) {
  const minLength = query.minLength ?? DEFAULT_MIN_STRING_LENGTH;
  const includeOffsets = query.includeOffsets ?? false;
  const result = await extractStrings(path, minLength, includeOffsets);

  if (!result.success) {
    return createErrorResult(
      result.error ?? 'strings extraction failed',
      query,
      {
        customHints: [
          'Ensure the "strings" CLI is installed (binutils on Linux, available via brew on macOS).',
        ],
      }
    );
  }

  const all = result.strings ?? [];
  const limited = all.slice(0, DEFAULT_STRING_LIMIT);

  return {
    status: 'success' as const,
    mode: 'strings' as const,
    path,
    strings: limited,
    totalFound: result.totalFound ?? 0,
    returned: limited.length,
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
    case 'identify':
      return handleIdentify(filePath, query);
    case 'list':
      return handleList(filePath, query);
    case 'extract':
      return handleExtract(filePath, query);
    case 'decompress':
      return handleDecompress(filePath, query);
    case 'strings':
      return handleStrings(filePath, query);
    default:
      return createErrorResult(
        `Unknown mode: ${String((query as BinaryInspectQuery).mode)}`,
        query
      );
  }
}
