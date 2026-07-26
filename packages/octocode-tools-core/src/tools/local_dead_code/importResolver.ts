import { posix } from 'node:path';

// Relative + index-file resolution only — the same known blind spot the
// prior OQL-era analyze pipeline documented: no tsconfig `paths`, no
// workspace aliases, no bundler resolution. A bare specifier ("zod",
// "@scope/pkg") is always external and never linked; only relative/absolute
// specifiers ("./x", "../x", "/x") are resolved against files this scan
// actually walked, so a link never crosses into node_modules.
const RESOLVE_EXTENSIONS = ['.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs'];

// TS-ESM convention: source imports a `.js`/`.jsx`/`.mjs`/`.cjs` specifier
// (Node ESM requires an explicit extension) even though only the `.ts`
// source file exists on disk — `foo.js` never exists, `foo.ts` does.
const TS_SOURCE_SWAP: Record<string, string> = {
  '.js': '.ts',
  '.jsx': '.tsx',
  '.mjs': '.mts',
  '.cjs': '.cts',
};

function normalizeSlashes(p: string): string {
  return p.split('\\').join('/');
}

/**
 * Resolve an import specifier to a relative path already present in
 * `knownFiles`, or `null` when it's external/bare or doesn't resolve within
 * the scanned set (dangling import, or a target outside the scan scope).
 */
export function resolveImportSpecifier(
  specifier: string,
  importingFileRelativePath: string,
  knownFiles: ReadonlySet<string>
): string | null {
  const normalizedSpecifier = normalizeSlashes(specifier);
  if (
    !normalizedSpecifier.startsWith('.') &&
    !normalizedSpecifier.startsWith('/')
  ) {
    return null;
  }

  const importingDir = posix.dirname(
    normalizeSlashes(importingFileRelativePath)
  );
  const joined = normalizedSpecifier.startsWith('/')
    ? normalizedSpecifier.slice(1)
    : posix.normalize(posix.join(importingDir, normalizedSpecifier));

  const candidates: string[] = [];
  const hasExtension = RESOLVE_EXTENSIONS.some(ext => joined.endsWith(ext));
  if (hasExtension) {
    candidates.push(joined);
    for (const [from, to] of Object.entries(TS_SOURCE_SWAP)) {
      if (joined.endsWith(from)) {
        candidates.push(joined.slice(0, -from.length) + to);
        break;
      }
    }
  } else {
    candidates.push(joined);
    for (const ext of RESOLVE_EXTENSIONS) {
      candidates.push(`${joined}${ext}`);
    }
    for (const ext of RESOLVE_EXTENSIONS) {
      candidates.push(posix.join(joined, `index${ext}`));
    }
  }

  for (const candidate of candidates) {
    if (knownFiles.has(candidate)) return candidate;
  }
  return null;
}
