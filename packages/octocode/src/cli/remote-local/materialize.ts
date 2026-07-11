import path from 'node:path';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';
import { refLabel, type GithubRef } from '../routing.js';
import { directToolText, parseCloneResult, parseFetchResult } from './parse.js';
import {
  cloneSparsePathFor,
  locationKindFor,
  normalizeRepoPath,
  resolveRepoOption,
} from './path-utils.js';
import type {
  DirectToolResult,
  FetchDirectoryData,
  RemoteMaterialization,
  RemoteMaterializationKind,
  RemoteMaterializationRequest,
} from './types.js';

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
