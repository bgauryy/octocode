/**
 * Single-stream decompression.
 * Backs mode="decompress" in localBinaryInspect.
 *
 * Handles: .gz .bz2 .xz .lzma .zst .lz4 .br .lzfse
 * Does NOT handle multi-entry archives (.tar.gz, .zip etc.) — those belong to archiveOps.
 */

import { safeExec } from '../../utils/exec/safe.js';
import type { ExecResult } from '../../utils/core/types.js';

// ─── types ────────────────────────────────────────────────────────────────────

export type DecompressFormat =
  | 'auto'
  | 'gzip'
  | 'bzip2'
  | 'xz'
  | 'lzma'
  | 'zstd'
  | 'lz4'
  | 'brotli'
  | 'lzfse';

export interface DecompressResult {
  success: boolean;
  content?: string;
  format?: Exclude<DecompressFormat, 'auto'>;
  backend?: string;
  error?: string;
}

// ─── backend map ──────────────────────────────────────────────────────────────

interface Backend {
  command: string;
  args: (path: string) => string[];
  fallback?: { command: string; args: (path: string) => string[] };
}

const BACKENDS: Record<Exclude<DecompressFormat, 'auto'>, Backend> = {
  gzip: {
    command: 'zcat',
    args: p => [p],
    fallback: { command: 'gunzip', args: p => ['-c', p] },
  },
  bzip2: { command: 'bzcat', args: p => [p] },
  xz: { command: 'xzcat', args: p => [p] },
  lzma: { command: 'xzcat', args: p => ['--format=lzma', p] },
  zstd: {
    command: 'zstdcat',
    args: p => [p],
    fallback: { command: 'zstd', args: p => ['-dcq', p] },
  },
  lz4: { command: 'lz4cat', args: p => [p] },
  brotli: { command: 'brotli', args: p => ['-dc', p] },
  lzfse: {
    command: 'lzfse',
    args: p => ['-decode', '-i', p, '-o', '/dev/stdout'],
  },
};

// ─── format detection ─────────────────────────────────────────────────────────

const EXT_MAP: Array<{
  ext: string;
  format: Exclude<DecompressFormat, 'auto'>;
}> = [
  { ext: '.gz', format: 'gzip' },
  { ext: '.bz2', format: 'bzip2' },
  { ext: '.xz', format: 'xz' },
  { ext: '.lzma', format: 'lzma' },
  { ext: '.zst', format: 'zstd' },
  { ext: '.zstd', format: 'zstd' },
  { ext: '.lz4', format: 'lz4' },
  { ext: '.br', format: 'brotli' },
  { ext: '.lzfse', format: 'lzfse' },
];

const MIME_MAP: Record<string, Exclude<DecompressFormat, 'auto'>> = {
  'application/gzip': 'gzip',
  'application/x-gzip': 'gzip',
  'application/x-bzip2': 'bzip2',
  'application/x-bzip': 'bzip2',
  'application/x-xz': 'xz',
  'application/x-lzma': 'lzma',
  'application/zstd': 'zstd',
  'application/x-zstd': 'zstd',
  'application/x-lz4': 'lz4',
  'application/x-brotli': 'brotli',
  'application/x-lzfse': 'lzfse',
};

function detectFromExt(
  path: string
): Exclude<DecompressFormat, 'auto'> | undefined {
  const lower = path.toLowerCase();
  for (const { ext, format } of EXT_MAP) {
    if (lower.endsWith(ext)) return format;
  }
  return undefined;
}

async function detectFromMime(
  path: string
): Promise<Exclude<DecompressFormat, 'auto'> | undefined> {
  const r: ExecResult = await safeExec('file', ['--mime-type', '-b', path]);
  if (!r.success) return undefined;
  return MIME_MAP[r.stdout.trim().toLowerCase()];
}

// ─── runner ───────────────────────────────────────────────────────────────────

async function runBackend(
  backend: Backend,
  path: string
): Promise<{
  success: boolean;
  stdout: string;
  stderr: string;
  commandUsed: string;
}> {
  const attempts = [
    { command: backend.command, args: backend.args(path) },
    ...(backend.fallback
      ? [
          {
            command: backend.fallback.command,
            args: backend.fallback.args(path),
          },
        ]
      : []),
  ];

  for (const attempt of attempts) {
    const r: ExecResult = await safeExec(attempt.command, attempt.args);
    if (r.success)
      return {
        success: true,
        stdout: r.stdout,
        stderr: r.stderr,
        commandUsed: attempt.command,
      };
  }

  const last = attempts[attempts.length - 1]!;
  const r: ExecResult = await safeExec(last.command, last.args);
  return {
    success: false,
    stdout: '',
    stderr: r.stderr,
    commandUsed: last.command,
  };
}

// ─── public API ───────────────────────────────────────────────────────────────

export async function decompressFile(
  path: string,
  format: DecompressFormat
): Promise<DecompressResult> {
  let resolvedFormat: Exclude<DecompressFormat, 'auto'> | undefined;

  if (format !== 'auto') {
    resolvedFormat = format;
  } else {
    resolvedFormat = detectFromExt(path) ?? (await detectFromMime(path));
  }

  if (!resolvedFormat) {
    return {
      success: false,
      error:
        'Could not detect compression format from extension or mime-type. ' +
        'Set format: gzip|bzip2|xz|lzma|zstd|lz4|brotli|lzfse explicitly.',
    };
  }

  const backend = BACKENDS[resolvedFormat];
  const result = await runBackend(backend, path);

  if (!result.success) {
    return {
      success: false,
      format: resolvedFormat,
      backend: result.commandUsed,
      error: result.stderr || `${result.commandUsed} failed`,
    };
  }

  return {
    success: true,
    format: resolvedFormat,
    backend: result.commandUsed,
    content: result.stdout,
  };
}
