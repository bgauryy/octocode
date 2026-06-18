/**
 * Archive list/extract operations.
 * Backs mode="list" and mode="extract" in localBinaryInspect.
 *
 * Backend selection (falls through on missing CLI):
 *   .aar/.yaa          → aa only
 *   .dmg/.rar/.cpgz    → 7z → 7zz → bsdtar
 *   .7z/.iso/.cab/…    → bsdtar → 7z → 7zz
 *   .tar.*             → tar → bsdtar → 7z
 *   default (zip-fam.) → unzip → tar → bsdtar → 7z
 */

import { safeExec } from '../../utils/exec/safe.js';
import type { ExecResult } from '../../utils/core/types.js';

// ─── types ────────────────────────────────────────────────────────────────────

export interface ArchiveChainResult {
  success: boolean;
  stdout: string;
  stderr: string;
  commandUsed?: string;
  missingCommands?: string[];
}

interface ChainAttempt {
  command: string;
  args: string[];
}

// ─── extension helpers ────────────────────────────────────────────────────────

const TAR_EXTS = [
  '.tar',
  '.tar.gz',
  '.tgz',
  '.tar.bz2',
  '.tbz2',
  '.tbz',
  '.tar.xz',
  '.txz',
  '.tar.zst',
  '.tzst',
];
const BSDTAR_NATIVE_EXTS = [
  '.7z',
  '.iso',
  '.cab',
  '.cpio',
  '.xar',
  '.pkg',
  '.ar',
  '.deb',
  '.lha',
  '.lzh',
];
const SEVENZIP_NATIVE_EXTS = ['.dmg', '.rar', '.cpgz'];
const APPLE_ARCHIVE_EXTS = ['.aar', '.yaa'];

function is(exts: string[], path: string): boolean {
  const lower = path.toLowerCase();
  return exts.some(e => lower.endsWith(e));
}

// ─── backend chain builders ───────────────────────────────────────────────────

function listChain(path: string, verbose: boolean): ChainAttempt[] {
  const tarArgs = verbose ? ['-tvf', path] : ['-tf', path];
  const unzipArgs = verbose ? ['-l', path] : ['-Z', '-1', path];
  const bsdtarArgs = tarArgs;
  const sevenzArgs = ['l', '-ba', '-bd', path];
  const aaArgs = verbose ? ['list', '-i', path, '-v'] : ['list', '-i', path];

  const tar = { command: 'tar', args: tarArgs };
  const unzip = { command: 'unzip', args: unzipArgs };
  const bsdtar = { command: 'bsdtar', args: bsdtarArgs };
  const sevenz = { command: '7z', args: sevenzArgs };
  const sevenzz = { command: '7zz', args: sevenzArgs };
  const aa = { command: 'aa', args: aaArgs };

  if (is(APPLE_ARCHIVE_EXTS, path)) return [aa];
  if (is(SEVENZIP_NATIVE_EXTS, path))
    return [sevenz, sevenzz, bsdtar, unzip, tar];
  if (is(BSDTAR_NATIVE_EXTS, path))
    return [bsdtar, sevenz, sevenzz, unzip, tar];
  if (is(TAR_EXTS, path)) return [tar, unzip, bsdtar, sevenz, sevenzz];
  return [unzip, tar, bsdtar, sevenz, sevenzz];
}

function extractChain(path: string, entry: string): ChainAttempt[] {
  // Each backend streams to stdout without touching the filesystem.
  const tar = { command: 'tar', args: ['-xOf', path, '--', entry] };
  const unzip = { command: 'unzip', args: ['-p', path, entry] };
  const bsdtar = { command: 'bsdtar', args: ['-xOf', path, '--', entry] };
  const sevenzArgs = ['e', '-so', '-bd', '--', path, entry];
  const sevenz = { command: '7z', args: sevenzArgs };
  const sevenzz = { command: '7zz', args: sevenzArgs };

  if (is(SEVENZIP_NATIVE_EXTS, path))
    return [sevenz, sevenzz, bsdtar, unzip, tar];
  if (is(BSDTAR_NATIVE_EXTS, path))
    return [bsdtar, sevenz, sevenzz, unzip, tar];
  if (is(TAR_EXTS, path)) return [tar, unzip, bsdtar, sevenz, sevenzz];
  return [unzip, tar, bsdtar, sevenz, sevenzz];
}

// ─── runner ───────────────────────────────────────────────────────────────────

async function runChain(chain: ChainAttempt[]): Promise<ArchiveChainResult> {
  const missing: string[] = [];

  for (const attempt of chain) {
    const result: ExecResult = await safeExec(attempt.command, attempt.args);

    if (!result.success) {
      // Distinguish "command not found" from "command failed"
      if (
        result.stderr.toLowerCase().includes('not found') ||
        result.stderr.toLowerCase().includes('no such file') ||
        result.code === 127
      ) {
        missing.push(attempt.command);
        continue;
      }
      return {
        success: false,
        stdout: result.stdout,
        stderr: result.stderr,
        commandUsed: attempt.command,
        missingCommands: missing.length ? missing : undefined,
      };
    }

    return {
      success: true,
      stdout: result.stdout,
      stderr: result.stderr,
      commandUsed: attempt.command,
      missingCommands: missing.length ? missing : undefined,
    };
  }

  return {
    success: false,
    stdout: '',
    stderr: 'All backends failed or were not found',
    missingCommands: missing,
  };
}

// ─── 7z output normalizer ─────────────────────────────────────────────────────

function normalize7zListing(stdout: string, command: string): string {
  if (command !== '7z' && command !== '7zz') return stdout;
  return stdout
    .split('\n')
    .map(line => {
      if (!line.trim()) return '';
      const m = line.match(/^.+\s{2,}(\S.*)$/);
      return m ? m[1]!.trim() : line.trim();
    })
    .filter(Boolean)
    .join('\n');
}

// ─── public API ───────────────────────────────────────────────────────────────

export async function listArchiveEntries(
  path: string,
  verbose: boolean
): Promise<ArchiveChainResult & { entries?: string[] }> {
  const result = await runChain(listChain(path, verbose));
  if (!result.success) return result;

  const normalized =
    !verbose && result.commandUsed
      ? normalize7zListing(result.stdout, result.commandUsed)
      : result.stdout;

  const entries = normalized
    .split('\n')
    .map(l => l.trim())
    .filter(Boolean);

  return { ...result, entries };
}

export async function extractArchiveEntry(
  path: string,
  entry: string
): Promise<ArchiveChainResult> {
  return runChain(extractChain(path, entry));
}
