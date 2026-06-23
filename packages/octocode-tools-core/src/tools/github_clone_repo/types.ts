export type CacheSource = 'clone' | 'directoryFetch' | 'treeFetch';

export interface CloneCacheMeta {
  clonedAt: string;
  expiresAt: string;
  owner: string;
  repo: string;
  branch: string;
  /** HEAD commit SHA at the time this entry was written. Absent on legacy entries. */
  commitSha?: string;
  sparsePath?: string;
  source: CacheSource;
  sizeBytes?: number;
}

export interface CloneRepoResult {
  localPath: string;
  cached: boolean;
  owner: string;
  repo: string;
  branch: string;
  sparsePath?: string;
}
