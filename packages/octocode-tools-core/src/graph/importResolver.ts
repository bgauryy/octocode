import { posix } from 'node:path';

// Shared relative + index-file resolution. Deliberately excludes tsconfig
// `paths`, custom aliases, node_modules, and bundler-specific resolution.
// Bare specifiers resolve only when the repository declares an exact package
// export and that export maps back to a source file in the scanned workspace.
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
  knownFiles: ReadonlySet<string>,
  workspacePackageExports: ReadonlyMap<string, string> = new Map()
): string | null {
  const normalizedSpecifier = normalizeSlashes(specifier);
  if (
    !normalizedSpecifier.startsWith('.') &&
    !normalizedSpecifier.startsWith('/')
  ) {
    return workspacePackageExports.get(normalizedSpecifier) ?? null;
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
