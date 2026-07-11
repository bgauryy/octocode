import { promises as fs } from 'fs';
import { join, basename, dirname, resolve, sep } from 'node:path';
import { securityRegistry } from '@octocodeai/octocode-engine/registry';
import { TOOL_NAMES } from '../../toolMetadata/proxies.js';
import { paths } from '../../../shared/paths.js';
import { contextUtils } from '../../../utils/contextUtils.js';
import { applyPagination } from '../../../utils/pagination/core.js';
import type { BinaryInspectQuery } from '../scheme.js';
import { buildNextPageContinuation } from '../../../scheme/pagination.js';

export const TOOL_NAME = TOOL_NAMES.LOCAL_BINARY_INSPECT;

function timestampForPath(date = new Date()): string {
  return date.toISOString().replace(/[:.]/g, '-');
}

export function unpackDestination(path: string): string {
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

export async function writeDerivedTextFile(
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
export function registerBinaryCommands(): void {
  if (binaryCommandsRegistered) return;
  try {
    securityRegistry.addAllowedCommands(BINARY_BACKEND_COMMANDS);
  } catch {
    /* ignore — validation will surface a clear error if a command is blocked */
  }
  binaryCommandsRegistered = true;
}

// ─── content helpers ──────────────────────────────────────────────────────────

export function filterByMatchString(
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

export interface ContentCharPagination {
  currentPage: number;
  totalPages: number;
  hasMore: boolean;
  charOffset: number;
  charLength: number;
  totalChars: number;
  nextCharOffset?: number;
}

export function paginateContent(
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

export function attachCharContinuation(
  query: BinaryInspectQuery,
  path: string,
  mode: BinaryInspectQuery['mode'],
  paginated: {
    pagination?: ContentCharPagination;
  },
  options?: { nextScanOffset?: number; archiveFile?: string }
): Record<string, unknown> | undefined {
  const next: Record<string, ReturnType<typeof buildNextPageContinuation>> = {};
  if (
    paginated.pagination?.hasMore &&
    paginated.pagination.nextCharOffset !== undefined
  ) {
    next.continueChars = buildNextPageContinuation(
      TOOL_NAME,
      {
        ...query,
        path,
        mode,
        ...(options?.archiveFile ? { archiveFile: options.archiveFile } : {}),
        charOffset: paginated.pagination.nextCharOffset,
        charLength: paginated.pagination.charLength,
      } as Record<string, unknown>,
      'Continue the inline content window.'
    );
  }
  if (
    typeof options?.nextScanOffset === 'number' &&
    Number.isFinite(options.nextScanOffset)
  ) {
    next.nextScan = buildNextPageContinuation(
      TOOL_NAME,
      {
        ...query,
        path,
        mode,
        scanOffset: options.nextScanOffset,
      } as Record<string, unknown>,
      'Advance the strings scan window across the binary.'
    );
  }
  return Object.keys(next).length > 0 ? { next } : undefined;
}
