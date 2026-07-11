/**
 * Binary file inspection (format lane) — native, via octocode-engine.
 * Backs mode="inspect" and mode="strings" in localBinaryInspect.
 *
 * No GNU binutils/coreutils dependency: `goblin`-backed parsing and a native
 * string scanner replace the old `file` / `xxd` / `strings` shell-outs, so this
 * lane works on Windows and on distroless/Alpine Linux, returns parser-stable
 * structured data, and recovers UTF-16 strings GNU `strings -a` misses.
 */

import { contextUtils } from '../../utils/contextUtils.js';
import type {
  BinaryInspectInfo,
  BinaryStrings,
} from '@octocodeai/octocode-engine';

// ─── inspect ────────────────────────────────────────────────────────────────

export interface InspectResult {
  success: boolean;
  info?: BinaryInspectInfo;
  error?: string;
}

export async function inspectBinaryFile(path: string): Promise<InspectResult> {
  try {
    return {
      success: true,
      info: await contextUtils.inspectBinaryNative(path),
    };
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

// ─── strings ──────────────────────────────────────────────────────────────────

export interface StringsResult {
  success: boolean;
  strings?: string[];
  totalFound?: number;
  /** True when more of the file remains to scan beyond this window (follow nextScanOffset). */
  truncated?: boolean;
  /** Absolute byte offset for the next scan window, or undefined at EOF. */
  nextScanOffset?: number;
  error?: string;
}

export async function extractStrings(
  path: string,
  minLength: number,
  includeOffsets: boolean,
  scanOffset = 0
): Promise<StringsResult> {
  try {
    const result: BinaryStrings = await contextUtils.extractBinaryStringsNative(
      path,
      minLength,
      includeOffsets,
      scanOffset
    );
    return {
      success: true,
      strings: result.strings,
      totalFound: result.totalFound,
      truncated: result.truncated,
      nextScanOffset: result.nextScanOffset,
    };
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}
