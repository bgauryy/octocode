import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { z } from 'zod';

export const DIRECTORY_META_FILE = '.octocode-directory-meta.json';

const count = z.number().int().nonnegative();
const directoryCacheSchema = z.object({
  commitSha: z.string(),
  complete: z.boolean(),
  directoryEntryCount: count,
  eligibleFileCount: count,
  skipped: z.object({
    nonFile: count,
    oversized: count,
    binary: count,
    fileLimit: count,
    fetchFailed: count,
    totalSizeLimit: count,
    pathTraversal: count,
  }),
  files: z.array(z.object({ path: z.string(), size: count })),
});

type DirectoryCacheMetadata = z.infer<typeof directoryCacheSchema>;

export function readDirectoryCacheMetadata(
  dirPath: string,
  commitSha: string
): DirectoryCacheMetadata | undefined {
  try {
    const parsed = directoryCacheSchema.safeParse(
      JSON.parse(readFileSync(join(dirPath, DIRECTORY_META_FILE), 'utf8'))
    );
    if (parsed.success && parsed.data.commitSha === commitSha)
      return parsed.data;
  } catch {
    // Older or damaged cache entries have no proof of completeness.
  }
  return undefined;
}

export function writeDirectoryCacheMetadata(
  dirPath: string,
  metadata: DirectoryCacheMetadata
): void {
  writeFileSync(join(dirPath, DIRECTORY_META_FILE), JSON.stringify(metadata), {
    encoding: 'utf8',
    mode: 0o600,
  });
}
