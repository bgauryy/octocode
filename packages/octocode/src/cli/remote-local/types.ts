export type DirectToolResult = {
  readonly isError?: boolean;
  readonly structuredContent?: unknown;
  readonly content?: readonly {
    readonly type?: string;
    readonly text?: string;
  }[];
};

export type CloneResultData = {
  readonly localPath?: string;
  readonly resolvedBranch?: string;
  readonly cached?: boolean;
};

export type CloneStructuredContent = {
  readonly results?: readonly { readonly data?: CloneResultData }[];
};

export type FetchFileData = {
  readonly localPath?: string;
  readonly repoRoot?: string;
  readonly resolvedBranch?: string;
  readonly cached?: boolean;
};

export type FetchDirectoryData = {
  readonly localPath?: string;
  readonly repoRoot?: string;
  readonly resolvedBranch?: string;
  readonly cached?: boolean;
  readonly complete?: boolean;
  readonly verified?: boolean;
  readonly commitSha?: string;
  readonly hasSubdirectories?: boolean;
  readonly skippedSummary?: Record<string, number>;
};

export type FetchStructuredContent = {
  readonly results?: readonly {
    readonly data?: {
      readonly files?: readonly FetchFileData[];
      readonly directories?: readonly FetchDirectoryData[];
    };
    readonly files?: readonly FetchFileData[];
    readonly directories?: readonly FetchDirectoryData[];
  }[];
};

export type RemoteMaterializationKind = 'file' | 'tree' | 'repo';

export type RemoteLocationKind = 'file' | 'directory' | 'repo' | 'tree';

/**
 * Structured, machine-readable description of where remote content was
 * materialized on disk. Replaces the prose `hints[]` the CLI used to emit:
 * agents should read these typed fields directly rather than parse sentences.
 */
export type RemoteLocation = {
  readonly kind: RemoteLocationKind;
  readonly localPath: string;
  readonly repoRoot?: string;
  readonly requestedPath?: string;
  readonly source?: 'clone' | 'tree';
  readonly cached?: boolean;
  readonly complete?: boolean;
  readonly verified?: boolean;
  readonly commitSha?: string;
  readonly hasSubdirectories?: boolean;
  readonly skippedSummary?: Record<string, number>;
  readonly resolvedBranch?: string;
};

export type RemoteMaterialization = {
  readonly owner: string;
  readonly repo: string;
  readonly branch?: string;
  readonly requestedPath: string;
  readonly localPath: string;
  readonly repoRoot: string;
  readonly source: 'clone' | 'tree';
  readonly complete: boolean;
  readonly verified: boolean;
  readonly commitSha?: string;
  readonly hasSubdirectories?: boolean;
  readonly skippedSummary?: Record<string, number>;
  readonly cached: boolean;
  readonly location: RemoteLocation;
};

export type HintableToolResult = {
  readonly isError?: boolean;
  readonly structuredContent?: unknown;
  readonly content?: readonly {
    readonly type?: string;
    readonly text?: string;
  }[];
};

export type RemoteMaterializationRequest = {
  readonly repoRef: string;
  readonly path?: string;
  readonly branch?: string;
  readonly forceRefresh?: boolean;
  readonly kind: RemoteMaterializationKind;
};
