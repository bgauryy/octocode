export type CacheSource = 'clone' | 'treeFetch';

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
  /** Immutable materialization generation selected by this atomic cache record. */
  snapshotId?: string;
}

export interface CloneRepoResult {
  localPath: string;
  cached: boolean;
  commitSha: string;
  /** Fresh checkout verification; cached working-tree contents are not reverified. */
  verified: boolean;
  owner: string;
  repo: string;
  branch: string;
  sparsePath?: string;
}
