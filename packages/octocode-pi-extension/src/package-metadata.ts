import fs from 'node:fs';
import path from 'node:path';

const metadataCache = new Map<string, Record<string, unknown> | undefined>();

/** Package metadata is stable for the lifetime of an installed extension process. */
function readMetadata(baseDir: string): Record<string, unknown> | undefined {
  if (metadataCache.has(baseDir)) return metadataCache.get(baseDir);
  let metadata: Record<string, unknown> | undefined;
  try {
    const parsed: unknown = JSON.parse(fs.readFileSync(path.join(path.dirname(baseDir), 'package.json'), 'utf8'));
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) metadata = parsed as Record<string, unknown>;
  } catch {
    // Metadata can be unavailable in an unpacked development tree.
  }
  metadataCache.set(baseDir, metadata);
  return metadata;
}

export function readOwnVersion(baseDir = import.meta.dirname): string | undefined {
  const value = readMetadata(baseDir)?.['version'];
  return typeof value === 'string' && value.trim() ? value.trim() : undefined;
}

export function readOwnDependencyVersion(name: string, baseDir = import.meta.dirname): string | undefined {
  const dependencies = readMetadata(baseDir)?.['dependencies'];
  if (!dependencies || typeof dependencies !== 'object' || Array.isArray(dependencies)) return undefined;
  const value = (dependencies as Record<string, unknown>)[name];
  return typeof value === 'string' && value.trim() ? value.trim() : undefined;
}
