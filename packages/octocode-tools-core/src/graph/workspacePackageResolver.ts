import { readFileSync } from 'node:fs';
import { join, posix } from 'node:path';
import {
  selectPackageExport,
  type PackageExportMode,
} from './packageExportTargets.js';
import type { ImportResolution } from './types.js';

interface PackageManifest {
  name?: unknown;
  exports?: unknown;
}

const OUTPUT_DIRECTORIES = new Set(['dist', 'out', 'lib', 'build']);
const SOURCE_EXTENSION_SWAPS: Record<string, string[]> = {
  '.js': ['.ts', '.tsx'],
  '.jsx': ['.tsx'],
  '.mjs': ['.mts', '.ts'],
  '.cjs': ['.cts', '.ts'],
  '.d.ts': ['.ts'],
};

function sourceCandidates(packageDir: string, target: string): string[] {
  if (!target.startsWith('./')) return [];
  const parts = target.slice(2).split('/');
  const variants = [parts];
  if (OUTPUT_DIRECTORIES.has(parts[0] ?? '')) {
    variants.push(['src', ...parts.slice(1)]);
  }

  const candidates = new Set<string>();
  for (const variant of variants) {
    const base = posix.normalize(posix.join(packageDir, ...variant));
    candidates.add(base);
    for (const [extension, replacements] of Object.entries(
      SOURCE_EXTENSION_SWAPS
    )) {
      if (!base.endsWith(extension)) continue;
      for (const replacement of replacements) {
        candidates.add(base.slice(0, -extension.length) + replacement);
      }
    }
  }
  return [...candidates];
}

export interface WorkspacePackageExports {
  resolve(
    specifier: string,
    mode: PackageExportMode
  ): ImportResolution | undefined;
}

/** Package export selection precedes bounded source projection and file existence checks. */
export function buildWorkspacePackageExports(
  rootAbsolutePath: string,
  knownFiles: ReadonlySet<string>
): WorkspacePackageExports {
  const packageDirs = new Set<string>(['.']);
  for (const file of knownFiles) {
    let directory = posix.dirname(file);
    while (directory !== '.') {
      packageDirs.add(directory);
      directory = posix.dirname(directory);
    }
  }

  const packages = new Map<
    string,
    { directory: string; exports: unknown } | null
  >();
  for (const packageDir of packageDirs) {
    let manifest: PackageManifest;
    try {
      manifest = JSON.parse(
        readFileSync(join(rootAbsolutePath, packageDir, 'package.json'), 'utf8')
      ) as PackageManifest;
    } catch {
      continue;
    }
    if (!manifest || typeof manifest.name !== 'string') continue;

    packages.set(
      manifest.name,
      packages.has(manifest.name)
        ? null
        : { directory: packageDir, exports: manifest.exports }
    );
  }
  const cache = new Map<string, ImportResolution>();
  return {
    resolve(specifier, mode) {
      const parts = specifier.split('/');
      const nameParts = specifier.startsWith('@') ? 2 : 1;
      const name = parts.slice(0, nameParts).join('/');
      if (!packages.has(name)) return undefined;
      const key = `${mode}\0${specifier}`;
      const cached = cache.get(key);
      if (cached) return cached;
      const item = packages.get(name);
      const suffix = parts.slice(nameParts).join('/');
      const selected = item
        ? selectPackageExport(
            item.exports,
            parts.length > nameParts ? `./${suffix}` : '.',
            mode
          )
        : { kind: 'unsupported' as const };
      const source =
        selected.kind === 'target' && item
          ? sourceCandidates(item.directory, selected.target).find(candidate =>
              knownFiles.has(candidate)
            )
          : undefined;
      const resolution: ImportResolution = source
        ? { target: source, status: 'resolved' }
        : {
            target: null,
            status:
              selected.kind === 'invalid' || selected.kind === 'unsupported'
                ? 'unsupported'
                : 'unresolvedInternal',
            reason: !item
              ? 'ambiguous-workspace-package'
              : item.exports === undefined
                ? 'package-exports-unavailable'
                : selected.kind === 'target'
                  ? 'package-export-target-unavailable'
                  : selected.kind === 'invalid'
                    ? 'invalid-package-exports'
                    : selected.kind === 'unsupported'
                      ? 'package-export-context-unavailable'
                      : 'package-subpath-not-exported',
          };
      cache.set(key, resolution);
      return resolution;
    },
  };
}
