export type CacheSource = 'clone' | 'directoryFetch';

export interface CloneCacheMeta {
  clonedAt: string;
  expiresAt: string;
  owner: string;
  repo: string;
  branch: string;
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
