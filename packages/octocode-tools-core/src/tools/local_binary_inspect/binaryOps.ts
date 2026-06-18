/**
 * Binary file inspection.
 * Backs mode="identify" and mode="strings" in localBinaryInspect.
 */

import { safeExec } from '../../utils/exec/safe.js';

// ─── identify ─────────────────────────────────────────────────────────────────

export interface IdentifyResult {
  success: boolean;
  fileType?: string;
  magicBytes?: string;
  error?: string;
}

export async function identifyFile(path: string): Promise<IdentifyResult> {
  const [typeResult, magicResult] = await Promise.all([
    safeExec('file', ['-b', path]),
    safeExec('xxd', ['-p', '-l', '32', path]),
  ]);

  if (!typeResult.success && !magicResult.success) {
    return { success: false, error: 'file and xxd both unavailable or failed' };
  }

  const fileType = typeResult.success ? typeResult.stdout.trim() : undefined;
  const rawHex = magicResult.success
    ? magicResult.stdout.replace(/\s+/g, '').trim()
    : undefined;
  const magicBytes = rawHex
    ? (rawHex.match(/.{1,2}/g) ?? []).join(' ')
    : undefined;

  return { success: true, fileType, magicBytes };
}

// ─── strings ──────────────────────────────────────────────────────────────────

export interface StringsResult {
  success: boolean;
  strings?: string[];
  totalFound?: number;
  error?: string;
}

export async function extractStrings(
  path: string,
  minLength: number,
  includeOffsets: boolean
): Promise<StringsResult> {
  // -a: whole file (default on ELF only scans loadable sections, missing embedded data)
  // -t x: hex byte offsets (if requested)
  const args = ['-a'];
  if (includeOffsets) args.push('-t', 'x');
  args.push('-n', String(minLength), path);

  const result = await safeExec('strings', args);

  if (!result.success) {
    return {
      success: false,
      error: result.stderr || 'strings failed — is the binary installed?',
    };
  }

  const all = result.stdout
    .split('\n')
    .map(l => l.trim())
    .filter(Boolean);

  if (all.length === 0) return { success: true, strings: [], totalFound: 0 };

  // Sort by length desc to surface longest (most meaningful) strings first
  const sorted = [...all].sort((a, b) => b.length - a.length);

  return { success: true, strings: sorted, totalFound: all.length };
}
