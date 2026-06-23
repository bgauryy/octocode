import type { CLICommand } from '../types.js';
import { getBool, getString, posIntOption } from '../options.js';
import { c, dim } from '../../utils/colors.js';
import { EXIT } from '../exit-codes.js';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';
import {
  markDirectToolFailure,
  printDirectToolResult,
  getDirectToolText,
} from './direct-tool-output.js';

type BinaryMode = 'inspect' | 'list' | 'extract' | 'decompress' | 'strings';

// Multi-entry archives → list. Checked before single-stream compression so
// that .tar.gz/.tgz route to "list", not "decompress".
const ARCHIVE_RE =
  /\.(zip|jar|war|ear|7z|deb|dmg|rpm|apk|nupkg|whl|gem|ar|a)$|\.tar(\.(gz|bz2|xz|zst|zstd|lz4|br|lzfse))?$|\.t(gz|bz2?|xz|zst)$/i;
// Single-stream compressed payloads → decompress.
const COMPRESSED_RE = /\.(gz|bz2|xz|lzma|zst|zstd|lz4|br|lzfse)$/i;

/**
 * The strings scan window covered only part of the file — returns the absolute
 * byte offset to continue from (lossless: no string is split across the
 * boundary), or undefined at EOF. The cursor rides in structuredContent (a
 * field), so a `binary --strings | grep` pipeline silently drops it; we surface
 * the continuation on stderr (below), which survives stdout piping.
 */
function nextScanOffset(result: {
  structuredContent?: unknown;
}): number | undefined {
  const sc = result?.structuredContent;
  if (!sc || typeof sc !== 'object') return undefined;
  const results = (sc as { results?: unknown }).results;
  if (!Array.isArray(results)) return undefined;
  for (const r of results) {
    if (r == null || typeof r !== 'object') continue;
    // The bulk envelope nests the handler payload under `data`; tolerate the
    // cursor sitting either on the result row or inside its `data`.
    const row = r as {
      nextScanOffset?: unknown;
      data?: { nextScanOffset?: unknown };
    };
    const n = row.nextScanOffset ?? row.data?.nextScanOffset;
    if (typeof n === 'number') return n;
  }
  return undefined;
}

/** Auto-pick the inspection mode from the file extension. */
function detectMode(file: string): BinaryMode {
  if (ARCHIVE_RE.test(file)) return 'list';
  if (COMPRESSED_RE.test(file)) return 'decompress';
  // Native binaries (.so/.dylib/.node/.exe/.wasm/…) and anything unrecognized
  // default to the smart inspect: it returns identity (type + magic) for any
  // file and structural detail for recognized executables. `strings` stays an
  // explicit opt-in.
  return 'inspect';
}

/** Explicit flag overrides win over auto-detection; --extract implies extract. */
function resolveMode(
  file: string,
  options: Record<string, string | boolean>
): { mode: BinaryMode; auto: boolean } {
  if (getString(options, 'extract')) return { mode: 'extract', auto: false };
  if (getBool(options, 'list')) return { mode: 'list', auto: false };
  if (getBool(options, 'strings')) return { mode: 'strings', auto: false };
  if (getBool(options, 'decompress'))
    return { mode: 'decompress', auto: false };
  if (getBool(options, 'inspect')) return { mode: 'inspect', auto: false };
  return { mode: detectMode(file), auto: true };
}

export const binaryCommand: CLICommand = {
  name: 'binary',
  options: [
    { name: 'list' },
    { name: 'extract', hasValue: true },
    { name: 'inspect' },
    { name: 'detailed' },
    { name: 'strings' },
    { name: 'decompress' },
    { name: 'match', hasValue: true },
    { name: 'min-length', hasValue: true },
    { name: 'max-entries', hasValue: true },
    { name: 'format', hasValue: true },
    { name: 'verbose' },
    { name: 'offsets' },
    { name: 'scan-offset', hasValue: true },
    { name: 'char-offset', hasValue: true },
    { name: 'char-length', hasValue: true },
    { name: 'page', hasValue: true },
    { name: 'json' },
  ],
  handler: async args => {
    const { options } = args;
    const file = args.args[0] ?? '';
    const jsonOutput = getBool(options, 'json');

    if (!file) {
      const error = 'Provide a file path to inspect.';
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error }));
      } else {
        console.error(`\n  ${c('red', '✗')} ${error}`);
        console.error(
          `\n  ${dim('Examples:')}\n` +
            `    binary app.zip                 ${dim('# list entries')}\n` +
            `    binary app.zip --extract README.md\n` +
            `    binary libssl.so               ${dim('# inspect: format, symbols, deps')}\n` +
            `    binary libssl.so --strings     ${dim('# readable strings')}\n` +
            `    binary data.json.gz            ${dim('# decompress')}\n`
        );
      }
      process.exitCode = EXIT.USAGE;
      return;
    }

    const { mode, auto } = resolveMode(file, options);
    const match = getString(options, 'match') || undefined;

    const query: Record<string, unknown> = {
      path: file,
      mode,
      mainResearchGoal: `Inspect ${file}`,
      researchGoal: `Run localBinaryInspect mode "${mode}" on ${file}`,
      reasoning: 'CLI binary command',
    };

    if (mode === 'list') {
      if (getBool(options, 'verbose')) query.verbose = true;
      const maxEntries = posIntOption(getString(options, 'max-entries'));
      if (maxEntries) query.maxEntries = maxEntries;
    } else if (mode === 'inspect') {
      if (getBool(options, 'detailed')) query.detailed = true;
    } else if (mode === 'extract') {
      query.archiveFile = getString(options, 'extract');
      if (match) query.matchString = match;
    } else if (mode === 'decompress') {
      const format = getString(options, 'format') || undefined;
      if (format) query.format = format;
      if (match) query.matchString = match;
    } else if (mode === 'strings') {
      const minLength = posIntOption(getString(options, 'min-length'));
      if (minLength) query.minLength = minLength;
      if (getBool(options, 'offsets')) query.includeOffsets = true;
      const scanOffsetRaw = getString(options, 'scan-offset');
      if (scanOffsetRaw && /^\d+$/.test(scanOffsetRaw)) {
        query.scanOffset = parseInt(scanOffsetRaw, 10);
      }
    }

    // Char-window pagination for the text-producing modes (strings/decompress/
    // extract); agents follow the charOffset=N hint to page losslessly.
    if (mode === 'strings' || mode === 'decompress' || mode === 'extract') {
      const charOffsetRaw = getString(options, 'char-offset');
      const charOffset =
        charOffsetRaw && /^\d+$/.test(charOffsetRaw)
          ? parseInt(charOffsetRaw, 10)
          : undefined;
      const charLength = posIntOption(getString(options, 'char-length'));
      if (charOffset !== undefined) query.charOffset = charOffset;
      if (charLength) query.charLength = charLength;
    }

    const page = posIntOption(getString(options, 'page'));
    if (page) query.page = page;

    if (!jsonOutput) {
      const how = auto ? ` ${dim(`(auto: ${mode})`)}` : '';
      process.stderr.write(
        `  ${dim(`Inspecting ${file}`)}${how}${dim(' ...')}\n`
      );
    }

    try {
      const result = await executeDirectTool('localBinaryInspect', {
        queries: [query],
      });

      printDirectToolResult(result, jsonOutput);

      if (!jsonOutput && result.isError) {
        const errText = getDirectToolText(result);
        if (
          errText.includes('pathValidationFailed') ||
          errText.includes('outside allowed directories')
        ) {
          process.stderr.write(
            `\n  ${dim('Tip:')} To inspect files outside the project, set the ${dim('ALLOWED_PATHS')} env var:\n` +
              `      ${c('cyan', `ALLOWED_PATHS=/usr/bin:/usr/local/bin binary ${file}`)}\n`
          );
        }
      }

      markDirectToolFailure(result);

      // The strings scan covers one window; more of the file is reachable
      // losslessly via --scan-offset. Surface the continuation on stderr so it
      // reaches the terminal even when stdout is piped through grep (the JSON
      // consumer reads nextScanOffset itself, so stay silent in --json mode).
      if (!jsonOutput) {
        const next = nextScanOffset(result);
        if (next !== undefined) {
          process.stderr.write(
            `\n  ${c('yellow', '⚠')} More of the file remains to scan (this is one window, not the whole file). Continue losslessly — no string is split across the boundary:\n` +
              `      ${c('cyan', `binary ${file} --strings --scan-offset ${next}`)}\n`
          );
        }
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: false,
            error: `Octocode tool runtime failed: ${message}`,
          })
        );
      } else {
        console.error(
          `\n  ${c('red', '✗')} Octocode tool runtime failed: ${message}\n`
        );
      }
      process.exitCode = EXIT.TOOL;
    }
  },
};
