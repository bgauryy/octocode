import { readFileSync } from 'node:fs';
import { join, posix } from 'node:path';

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

function firstExportTarget(value: unknown): string | null {
  if (typeof value === 'string') return value;
  if (Array.isArray(value)) {
    for (const item of value) {
      const target = firstExportTarget(item);
      if (target) return target;
    }
    return null;
  }
  if (!value || typeof value !== 'object') return null;
  const conditions = value as Record<string, unknown>;
  for (const key of ['import', 'default', 'node', 'require', 'types']) {
    const target = firstExportTarget(conditions[key]);
    if (target) return target;
  }
  return null;
}

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

/** Resolve only exact package exports declared by manifests in this scan. */
export function buildWorkspacePackageExports(
  rootAbsolutePath: string,
  knownFiles: ReadonlySet<string>
): Map<string, string> {
  const packageDirs = new Set<string>(['.']);
  for (const file of knownFiles) {
    let directory = posix.dirname(file);
    while (directory !== '.') {
      packageDirs.add(directory);
      directory = posix.dirname(directory);
    }
  }

  const resolved = new Map<string, string>();
  for (const packageDir of packageDirs) {
    let manifest: PackageManifest;
    try {
      manifest = JSON.parse(
        readFileSync(join(rootAbsolutePath, packageDir, 'package.json'), 'utf8')
      ) as PackageManifest;
    } catch {
      continue;
    }
    if (typeof manifest.name !== 'string') continue;

    const exportEntries: Array<[string, unknown]> = [];
    if (
      manifest.exports &&
      typeof manifest.exports === 'object' &&
      !Array.isArray(manifest.exports) &&
      Object.keys(manifest.exports).some(key => key.startsWith('.'))
    ) {
      exportEntries.push(
        ...Object.entries(manifest.exports as Record<string, unknown>)
      );
    } else if (manifest.exports !== undefined) {
      exportEntries.push(['.', manifest.exports]);
    }

    for (const [exportKey, exportValue] of exportEntries) {
      if (exportKey !== '.' && !exportKey.startsWith('./')) continue;
      const target = firstExportTarget(exportValue);
      if (!target) continue;
      const source = sourceCandidates(packageDir, target).find(candidate =>
        knownFiles.has(candidate)
      );
      if (!source) continue;
      const specifier =
        exportKey === '.'
          ? manifest.name
          : `${manifest.name}/${exportKey.slice(2)}`;
      resolved.set(specifier, source);
    }
  }
  return resolved;
}
