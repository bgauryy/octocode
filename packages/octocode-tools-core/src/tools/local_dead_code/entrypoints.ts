import { readFileSync } from 'node:fs';
import { posix } from 'node:path';

const TEST_FILE_PATTERN = /(\.(test|spec)\.[^/]+$)|(^|\/)(__tests__|tests?)\//i;

const SCRIPT_FILE_EXTENSION = /\.(mjs|cjs|jsx?|tsx?)$/i;

/**
 * Pull file-looking tokens (e.g. `build.mjs` in `node build.mjs && tsc ...`)
 * out of a package.json `scripts` value. package.json `scripts` are the other
 * common way a file is "used" without ever being imported — a build/postinstall
 * script invoked directly via `node X.mjs` has no import edge pointing at it,
 * so without this it reads as dead despite being live.
 */
function extractScriptFileReferences(scriptValue: string): string[] {
  const refs: string[] = [];
  for (const rawToken of scriptValue.split(/\s+/)) {
    const candidates = rawToken.includes('=')
      ? [rawToken, rawToken.slice(rawToken.indexOf('=') + 1)]
      : [rawToken];
    for (const candidate of candidates) {
      const cleaned = candidate.replace(/^["']|["']$/g, '');
      if (SCRIPT_FILE_EXTENSION.test(cleaned) && !cleaned.startsWith('-')) {
        refs.push(cleaned);
      }
    }
  }
  return refs;
}

function normalizeSlashes(p: string): string {
  return p.split('\\').join('/');
}

function stripLeadingDotSlash(p: string): string {
  return p.replace(/^\.\//, '');
}

// Common build-output directory names package.json `main`/`exports`/`bin`
// point at, which a source scan excludes — see DEFAULT_DEAD_CODE_EXCLUDE_DIRS.
const BUILD_OUTPUT_DIRS = ['dist', 'build', 'out'];
const TS_EXTENSION_SWAP: Record<string, string> = {
  '.js': '.ts',
  '.jsx': '.tsx',
  '.mjs': '.mts',
  '.cjs': '.cts',
};

function swapToTsExtension(p: string): string | undefined {
  for (const [from, to] of Object.entries(TS_EXTENSION_SWAP)) {
    if (p.endsWith(from)) return p.slice(0, -from.length) + to;
  }
  return undefined;
}

/**
 * package.json commonly points at compiled output (`dist/foo.js`) rather than
 * the source that produced it. Try, in order: swapping the extension for its
 * TS source form, swapping a leading build-output dir for `src`, and both
 * together — the combination that actually matches this repo's dist/src
 * layout is unknown ahead of time, so every plausible one is tried.
 */
function findSourceEquivalent(
  normalized: string,
  knownFiles: ReadonlySet<string>
): string | undefined {
  const candidates: string[] = [];

  const extSwapped = swapToTsExtension(normalized);
  if (extSwapped) candidates.push(extSwapped);

  for (const outDir of BUILD_OUTPUT_DIRS) {
    const prefix = `${outDir}/`;
    if (!normalized.startsWith(prefix)) continue;
    const rest = normalized.slice(prefix.length);
    const srcPath = `src/${rest}`;
    candidates.push(srcPath);
    const srcExtSwapped = swapToTsExtension(srcPath);
    if (srcExtSwapped) candidates.push(srcExtSwapped);
  }

  return candidates.find(candidate => knownFiles.has(candidate));
}

/** Every string value nested anywhere in package.json's `exports`/`bin` shape. */
function collectStringLeaves(value: unknown, out: string[]): void {
  if (typeof value === 'string') {
    out.push(value);
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) collectStringLeaves(item, out);
    return;
  }
  if (value && typeof value === 'object') {
    for (const item of Object.values(value)) collectStringLeaves(item, out);
  }
}

/**
 * Resolve entrypoint files: explicit paths win; otherwise auto-detect from
 * package.json (main/exports/bin, plus file-looking tokens inside `scripts`
 * such as `node build.mjs`) plus, when `includeTests`, every file that looks
 * like a test. Tests count as roots because a symbol only exercised by a test
 * is proven live, not dead — treating them as entrypoints is the conservative
 * direction (never a false dead-code claim).
 *
 * Returns only entrypoints present in `knownFiles` — an explicit path that
 * doesn't exist in the scanned set is dropped and reported via `warnings`.
 */
export function resolveEntrypoints(
  rootAbsolutePath: string,
  explicitEntrypoints: string[] | undefined,
  includeTests: boolean,
  knownFiles: ReadonlySet<string>
): { entrypoints: string[]; warnings: string[] } {
  const warnings: string[] = [];
  const resolved = new Set<string>();

  if (explicitEntrypoints && explicitEntrypoints.length > 0) {
    for (const raw of explicitEntrypoints) {
      const normalized = stripLeadingDotSlash(normalizeSlashes(raw));
      if (knownFiles.has(normalized)) {
        resolved.add(normalized);
      } else {
        warnings.push(`entrypoint not found in scan: ${raw}`);
      }
    }
  } else {
    let pkg: Record<string, unknown> | undefined;
    try {
      const pkgPath = posix.join(rootAbsolutePath, 'package.json');
      pkg = JSON.parse(readFileSync(pkgPath, 'utf-8')) as Record<
        string,
        unknown
      >;
    } catch {
      // No package.json, or unreadable/invalid — fall through with no
      // package-derived entrypoints; caller sees this via an empty
      // entrypointsResolved plus the warning below.
    }

    if (pkg) {
      const leaves: string[] = [];
      collectStringLeaves(pkg.main, leaves);
      collectStringLeaves(pkg.exports, leaves);
      collectStringLeaves(pkg.bin, leaves);
      if (pkg.scripts && typeof pkg.scripts === 'object') {
        for (const scriptValue of Object.values(
          pkg.scripts as Record<string, unknown>
        )) {
          if (typeof scriptValue === 'string') {
            leaves.push(...extractScriptFileReferences(scriptValue));
          }
        }
      }
      for (const leaf of leaves) {
        const normalized = posix.normalize(
          stripLeadingDotSlash(normalizeSlashes(leaf))
        );
        const match = knownFiles.has(normalized)
          ? normalized
          : findSourceEquivalent(normalized, knownFiles);
        if (match) resolved.add(match);
      }
    }

    if (resolved.size === 0) {
      warnings.push(
        'no entrypoints resolved from package.json — pass `entrypoints` explicitly, or every export in a reachable file will read as unreachable'
      );
    }
  }

  if (includeTests) {
    for (const file of knownFiles) {
      if (TEST_FILE_PATTERN.test(file)) resolved.add(file);
    }
  }

  return { entrypoints: [...resolved], warnings };
}
