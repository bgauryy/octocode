import path from 'node:path';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';
import {
  resolveRef,
  isGithubRef,
  refLabel,
  type GithubRef,
} from './routing.js';

type DirectToolResult = {
  readonly isError?: boolean;
  readonly structuredContent?: unknown;
  readonly content?: readonly {
    readonly type?: string;
    readonly text?: string;
  }[];
};

type CloneResultData = {
  readonly localPath?: string;
  readonly resolvedBranch?: string;
  readonly cached?: boolean;
};

type CloneStructuredContent = {
  readonly results?: readonly { readonly data?: CloneResultData }[];
};

type FetchFileData = {
  readonly localPath?: string;
  readonly repoRoot?: string;
  readonly resolvedBranch?: string;
  readonly cached?: boolean;
};

type FetchDirectoryData = {
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

type FetchStructuredContent = {
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

type HintableToolResult = {
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

function directToolText(result: DirectToolResult): string {
  const text = (result.content ?? [])
    .filter(item => item.type === 'text' && typeof item.text === 'string')
    .map(item => item.text)
    .join('\n')
    .trim();
  if (text.length > 0) return text;
  return JSON.stringify(result.structuredContent ?? result, null, 2);
}

function parseCloneResult(result: DirectToolResult): CloneResultData {
  const structured = result.structuredContent as
    CloneStructuredContent | undefined;
  return structured?.results?.[0]?.data ?? {};
}

function parseFetchResult(
  result: DirectToolResult,
  kind: Extract<RemoteMaterializationKind, 'file' | 'tree'>
): FetchFileData | FetchDirectoryData {
  const structured = result.structuredContent as
    FetchStructuredContent | undefined;
  const first = structured?.results?.[0];
  const data = first?.data ?? first;
  if (kind === 'file') return data?.files?.[0] ?? {};
  return data?.directories?.[0] ?? {};
}

function normalizeRepoPath(...parts: readonly (string | undefined)[]): string {
  const joined = parts
    .map(part => part?.trim())
    .filter((part): part is string => Boolean(part && part !== '.'))
    .join('/');
  if (!joined) return '';
  if (path.posix.isAbsolute(joined)) {
    throw new Error('Remote path must be repository-relative.');
  }
  if (joined.split('/').some(segment => segment === '..')) {
    throw new Error('Remote path cannot contain path traversal segments.');
  }

  const normalized = path.posix.normalize(joined);
  if (
    normalized === '..' ||
    normalized.startsWith('../') ||
    normalized.includes('/../')
  ) {
    throw new Error('Remote path cannot contain path traversal segments.');
  }
  return normalized === '.' ? '' : normalized;
}

function cloneSparsePathFor(
  requestedPath: string,
  kind: RemoteMaterializationKind
): string | undefined {
  if (!requestedPath) return undefined;
  if (kind !== 'file') return requestedPath;
  const parent = path.posix.dirname(requestedPath);
  return parent === '.' ? undefined : parent;
}

function resolveRepoOption(repoRef: string, branch?: string): GithubRef {
  const ref = resolveRef(repoRef, branch || undefined);
  if (!isGithubRef(ref)) {
    throw new Error(`--repo must be a GitHub ref, got "${repoRef}".`);
  }
  return ref;
}

/**
 * Maps a materialization request kind to the structural `location.kind`.
 * A `tree` materialization lands on disk as a directory.
 */
function locationKindFor(kind: RemoteMaterializationKind): RemoteLocationKind {
  if (kind === 'tree') return 'directory';
  return kind;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function locationPayload(location: RemoteLocation): Record<string, unknown> {
  return {
    kind: location.kind,
    localPath: location.localPath,
    ...(location.repoRoot ? { repoRoot: location.repoRoot } : {}),
    ...(location.requestedPath
      ? { requestedPath: location.requestedPath }
      : {}),
    ...(location.source ? { source: location.source } : {}),
    ...(location.cached !== undefined ? { cached: location.cached } : {}),
    ...(location.complete !== undefined ? { complete: location.complete } : {}),
    ...(location.verified !== undefined ? { verified: location.verified } : {}),
    ...(location.commitSha ? { commitSha: location.commitSha } : {}),
    ...(location.hasSubdirectories ? { hasSubdirectories: true } : {}),
    ...(location.skippedSummary &&
    Object.keys(location.skippedSummary).length > 0
      ? { skippedSummary: location.skippedSummary }
      : {}),
    ...(location.resolvedBranch
      ? { resolvedBranch: location.resolvedBranch }
      : {}),
  };
}

/**
 * Renders the structured `location` of a materialization as a compact text
 * block. The shape mirrors `structuredContent.location` — agents should read
 * the typed fields, not parse prose hints (the old behavior).
 */
export function formatMaterializationHints(
  materialized: RemoteMaterialization
): string {
  const { location } = materialized;
  const lines = [
    'location:',
    `  kind: ${JSON.stringify(location.kind)}`,
    `  localPath: ${JSON.stringify(location.localPath)}`,
  ];
  if (location.repoRoot) {
    lines.push(`  repoRoot: ${JSON.stringify(location.repoRoot)}`);
  }
  if (location.requestedPath) {
    lines.push(`  requestedPath: ${JSON.stringify(location.requestedPath)}`);
  }
  if (location.source) {
    lines.push(`  source: ${JSON.stringify(location.source)}`);
  }
  if (location.resolvedBranch) {
    lines.push(`  resolvedBranch: ${JSON.stringify(location.resolvedBranch)}`);
  }
  if (location.cached !== undefined) {
    lines.push(`  cached: ${location.cached}`);
  }
  if (location.complete !== undefined) {
    lines.push(`  complete: ${location.complete}`);
  }
  if (location.verified !== undefined) {
    lines.push(`  verified: ${location.verified}`);
  }
  if (location.commitSha) {
    lines.push(`  commitSha: ${location.commitSha}`);
  }
  if (location.hasSubdirectories) {
    lines.push(`  hasSubdirectories: true`);
  }
  if (
    location.skippedSummary &&
    Object.keys(location.skippedSummary).length > 0
  ) {
    lines.push(`  skippedSummary: ${JSON.stringify(location.skippedSummary)}`);
  }
  return lines.join('\n');
}

/** Remote context stamped into every `next.*` query so agents following
 *  pagination to page 2+ can see the provenance even though localSearchCode
 *  won't re-emit the full `location` block. */
function remoteAnnotation(m: RemoteMaterialization): Record<string, unknown> {
  return {
    _remote: {
      owner: m.owner,
      repo: m.repo,
      source: m.source,
      localPath: m.localPath,
      ...(m.branch ? { branch: m.branch } : {}),
      ...(m.commitSha ? { commitSha: m.commitSha } : {}),
    },
  };
}

/** Walk results[*].data.next.* and inject _remote into each `query` object. */
function annotateNextPointers(
  sc: Record<string, unknown>,
  annotation: Record<string, unknown>
): Record<string, unknown> {
  const results = sc.results;
  if (!Array.isArray(results)) return sc;
  const patched = results.map(item => {
    if (!isRecord(item)) return item;
    const data = item.data;
    if (!isRecord(data)) return item;
    const next = data.next;
    if (!isRecord(next)) return item;
    const patchedNext = Object.fromEntries(
      Object.entries(next).map(([k, v]) => {
        if (!isRecord(v)) return [k, v];
        const query = v.query;
        if (!isRecord(query)) return [k, v];
        return [k, { ...v, query: { ...query, ...annotation } }];
      })
    );
    return { ...item, data: { ...data, next: patchedNext } };
  });
  return { ...sc, results: patched };
}

export function withMaterializationHints<T extends HintableToolResult>(
  result: T,
  materialized: RemoteMaterialization
): T {
  const structuredRecord = isRecord(result.structuredContent)
    ? result.structuredContent
    : { data: result.structuredContent };
  const withLocation = {
    ...structuredRecord,
    location: locationPayload(materialized.location),
  };
  const structuredContent = annotateNextPointers(
    withLocation,
    remoteAnnotation(materialized)
  );
  const locationBlock = formatMaterializationHints(materialized);
  const content = result.content?.map(item =>
    item.type === 'text' && typeof item.text === 'string'
      ? { ...item, text: `${item.text.trimEnd()}\n${locationBlock}` }
      : item
  );

  return {
    ...result,
    structuredContent,
    ...(content ? { content } : {}),
  };
}

export async function materializeRemoteForCli(
  request: RemoteMaterializationRequest
): Promise<RemoteMaterialization> {
  const repo = resolveRepoOption(request.repoRef, request.branch);
  const requestedPath = normalizeRepoPath(repo.subpath, request.path);
  if (request.kind === 'file' && !requestedPath) {
    throw new Error(
      'File materialization requires a repository-relative path.'
    );
  }

  if (request.kind === 'repo') {
    return materializeCloneForCli(repo, requestedPath, request);
  }

  return materializeTreeForCli(repo, requestedPath, request);
}

async function materializeCloneForCli(
  repo: GithubRef,
  requestedPath: string,
  request: RemoteMaterializationRequest
): Promise<RemoteMaterialization> {
  const cloneSparsePath = cloneSparsePathFor(requestedPath, request.kind);

  const result = (await executeDirectTool('ghCloneRepo', {
    queries: [
      {
        owner: repo.owner,
        repo: repo.repo,
        branch: repo.branch,
        sparsePath: cloneSparsePath,
        forceRefresh: request.forceRefresh || undefined,
        mainResearchGoal: 'Materialize GitHub content for local CLI research',
        researchGoal: `Save ${refLabel(repo)}${requestedPath ? `/${requestedPath}` : ''} locally`,
        reasoning: 'CLI remote-as-local materialization',
      },
    ],
  })) as DirectToolResult;

  if (result.isError) {
    throw new Error(directToolText(result));
  }

  const data = parseCloneResult(result);
  if (!data.localPath) {
    throw new Error('ghCloneRepo did not return a localPath.');
  }

  const repoRoot = path.resolve(data.localPath);
  const localPath = requestedPath
    ? path.resolve(repoRoot, ...requestedPath.split('/'))
    : repoRoot;
  const resolvedBranch = data.resolvedBranch ?? repo.branch;
  const cached = Boolean(data.cached);

  return {
    owner: repo.owner,
    repo: repo.repo,
    ...(resolvedBranch ? { branch: resolvedBranch } : {}),
    requestedPath,
    localPath,
    repoRoot,
    source: 'clone',
    complete: true,
    verified: true,
    cached,
    location: {
      kind: locationKindFor(request.kind),
      localPath,
      repoRoot,
      ...(requestedPath ? { requestedPath } : {}),
      source: 'clone',
      cached,
      complete: true,
      verified: true,
      ...(resolvedBranch ? { resolvedBranch } : {}),
    },
  };
}

async function materializeTreeForCli(
  repo: GithubRef,
  requestedPath: string,
  request: RemoteMaterializationRequest
): Promise<RemoteMaterialization> {
  const kind = request.kind as Extract<
    RemoteMaterializationKind,
    'file' | 'tree'
  >;
  const result = (await executeDirectTool('ghGetFileContent', {
    queries: [
      {
        owner: repo.owner,
        repo: repo.repo,
        branch: repo.branch,
        path: requestedPath,
        type: kind === 'file' ? 'file' : 'directory',
        forceRefresh: request.forceRefresh || undefined,
        ...(kind === 'file'
          ? { fullContent: true, contextLines: 0, minify: 'none' }
          : {}),
        mainResearchGoal: 'Materialize GitHub content for local CLI research',
        researchGoal: `Save ${refLabel(repo)}${requestedPath ? `/${requestedPath}` : ''} locally`,
        reasoning: 'CLI remote-as-local materialization',
      },
    ],
  })) as DirectToolResult;

  if (result.isError) {
    throw new Error(directToolText(result));
  }

  const data = parseFetchResult(result, kind);
  if (!data.localPath) {
    throw new Error('ghGetFileContent did not return a localPath.');
  }

  const localPath = path.resolve(data.localPath);
  const repoRoot = path.resolve(data.repoRoot ?? data.localPath);
  const resolvedBranch = data.resolvedBranch ?? repo.branch;
  const cached = Boolean(data.cached);
  // complete/verified/commitSha are only on FetchDirectoryData (type:'directory');
  // for single-file fetches the fields are absent and default to safe values.
  const dirData = data as FetchDirectoryData;
  const complete = dirData.complete ?? true;
  const verified = dirData.verified ?? false;
  const commitSha = dirData.commitSha;
  const hasSubdirectories = dirData.hasSubdirectories ?? false;
  const skippedSummary = dirData.skippedSummary;

  return {
    owner: repo.owner,
    repo: repo.repo,
    ...(resolvedBranch ? { branch: resolvedBranch } : {}),
    requestedPath,
    localPath,
    repoRoot,
    source: 'tree',
    complete,
    verified,
    ...(commitSha ? { commitSha } : {}),
    ...(hasSubdirectories ? { hasSubdirectories: true } : {}),
    ...(skippedSummary ? { skippedSummary } : {}),
    cached,
    location: {
      kind: locationKindFor(request.kind),
      localPath,
      repoRoot,
      ...(requestedPath ? { requestedPath } : {}),
      source: 'tree',
      cached,
      complete,
      verified,
      ...(commitSha ? { commitSha } : {}),
      ...(hasSubdirectories ? { hasSubdirectories: true } : {}),
      ...(skippedSummary ? { skippedSummary } : {}),
      ...(resolvedBranch ? { resolvedBranch } : {}),
    },
  };
}

export function isFullRepoOption(options: {
  readonly repo?: string;
  readonly owner?: string;
}): boolean {
  return Boolean(
    options.repo?.includes('/') || (!options.owner && options.repo)
  );
}
