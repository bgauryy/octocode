import path from 'node:path';
import { readFile, readdir } from 'node:fs/promises';
import {
  DEFAULT_EXCLUDE_DIRS,
  type Manifest,
  type ResearchDependencyIssue,
  type SourceFile,
} from './types.js';
import { recordValue, relative, stringValue } from './utils.js';

export async function walkFiles(
  root: string,
  maxFiles: number
): Promise<string[]> {
  const out: string[] = [];
  async function visit(dir: string): Promise<void> {
    if (out.length >= maxFiles) return;
    const entries = await readdir(dir, { withFileTypes: true });
    for (const entry of entries) {
      if (out.length >= maxFiles) return;
      if (entry.isDirectory()) {
        if (DEFAULT_EXCLUDE_DIRS.has(entry.name)) continue;
        await visit(path.join(dir, entry.name));
        continue;
      }
      if (entry.isFile()) out.push(path.join(dir, entry.name));
    }
  }
  await visit(root);
  return out;
}

export async function readManifests(
  paths: readonly string[]
): Promise<Manifest[]> {
  const manifests = await Promise.all(
    paths.map(async manifestPath => parseManifest(manifestPath))
  );
  return manifests.filter(
    (manifest): manifest is Manifest => manifest !== null
  );
}

async function parseManifest(manifestPath: string): Promise<Manifest | null> {
  const raw = await readJsonObject(manifestPath);
  if (!raw) return null;
  const dir = path.dirname(manifestPath);
  const deps = new Map<string, readonly string[]>();
  for (const field of [
    'dependencies',
    'devDependencies',
    'peerDependencies',
    'optionalDependencies',
  ]) {
    const values = recordValue(raw[field]);
    if (!values) continue;
    for (const name of Object.keys(values)) {
      deps.set(name, [...(deps.get(name) ?? []), field]);
    }
  }
  return {
    path: manifestPath,
    dir,
    name: stringValue(raw.name),
    deps,
    entrypoints: manifestEntrypoints(dir, raw),
  };
}

async function readJsonObject(
  file: string
): Promise<Record<string, unknown> | null> {
  try {
    const parsed = JSON.parse(await readFile(file, 'utf8')) as unknown;
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

function manifestEntrypoints(
  dir: string,
  raw: Record<string, unknown>
): readonly string[] {
  const candidates = new Set<string>();
  for (const field of ['main', 'module', 'types', 'typings']) {
    const value = stringValue(raw[field]);
    if (value) candidates.add(path.resolve(dir, value));
  }
  const bin = raw.bin;
  if (typeof bin === 'string') candidates.add(path.resolve(dir, bin));
  if (bin && typeof bin === 'object' && !Array.isArray(bin)) {
    for (const value of Object.values(bin)) {
      if (typeof value === 'string') candidates.add(path.resolve(dir, value));
    }
  }
  collectExportsEntrypoints(dir, raw.exports, candidates);
  for (const fallback of [
    'src/index.ts',
    'src/index.tsx',
    'src/index.js',
    'index.ts',
    'index.js',
    'src/lib.rs',
    'src/main.rs',
    'main.rs',
    '__init__.py',
    'main.py',
    'src/main.py',
    'main.go',
    'cmd/main.go',
    'src/main/java/Main.java',
  ]) {
    candidates.add(path.resolve(dir, fallback));
  }
  return [...candidates];
}

function collectExportsEntrypoints(
  dir: string,
  value: unknown,
  out: Set<string>
): void {
  if (typeof value === 'string') {
    out.add(path.resolve(dir, value));
    return;
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) return;
  for (const child of Object.values(value as Record<string, unknown>)) {
    collectExportsEntrypoints(dir, child, out);
  }
}

export function collectDependencyIssues(
  root: string,
  manifests: readonly Manifest[],
  sourceFiles: readonly SourceFile[],
  workspacePackages: ReadonlySet<string>
): readonly ResearchDependencyIssue[] {
  const byManifest = new Map<string, Set<string>>();
  const usedByPackage = new Map<string, Set<string>>();
  for (const file of sourceFiles) {
    const manifest = nearestManifest(file.path, manifests);
    if (!manifest) continue;
    const used = byManifest.get(manifest.path) ?? new Set<string>();
    byManifest.set(manifest.path, used);
    for (const packageName of file.externalPackages) {
      used.add(packageName);
      const files =
        usedByPackage.get(`${manifest.path}\0${packageName}`) ??
        new Set<string>();
      files.add(file.rel);
      usedByPackage.set(`${manifest.path}\0${packageName}`, files);
    }
  }

  const issues: ResearchDependencyIssue[] = [];
  for (const manifest of manifests) {
    const used = byManifest.get(manifest.path) ?? new Set<string>();
    for (const packageName of used) {
      if (
        workspacePackages.has(packageName) ||
        manifest.deps.has(packageName)
      ) {
        continue;
      }
      issues.push({
        kind: 'unlistedDependency',
        packageName,
        manifest: relative(root, manifest.path),
        usedBy: [
          ...(usedByPackage.get(`${manifest.path}\0${packageName}`) ?? []),
        ],
        declaredIn: [],
        verdict: 'unlisted-dependency',
      });
    }
    for (const [packageName, fields] of manifest.deps) {
      if (fields.length > 1) {
        issues.push({
          kind: 'duplicateDependency',
          packageName,
          manifest: relative(root, manifest.path),
          usedBy: [
            ...(usedByPackage.get(`${manifest.path}\0${packageName}`) ?? []),
          ],
          declaredIn: fields,
          verdict: 'duplicate-dependency',
        });
      }
      if (!used.has(packageName) && !workspacePackages.has(packageName)) {
        issues.push({
          kind: 'unusedDependency',
          packageName,
          manifest: relative(root, manifest.path),
          usedBy: [],
          declaredIn: fields,
          verdict: 'candidate-unused-dependency',
        });
      }
    }
  }
  return issues;
}

function nearestManifest(
  file: string,
  manifests: readonly Manifest[]
): Manifest | undefined {
  const candidates = manifests
    .filter(manifest => file.startsWith(`${manifest.dir}${path.sep}`))
    .sort((a, b) => b.dir.length - a.dir.length);
  return candidates[0];
}
