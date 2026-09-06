import path from 'node:path';
import { statSync } from 'node:fs';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';
import { refLabel, type GithubRef } from '../routing.js';
import { directToolText, parseCloneResult, parseFetchResult } from './parse.js';
import {
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
  const result = (await executeDirectTool('ghCloneRepo', {
    queries: [
      {
        owner: repo.owner,
        repo: repo.repo,
        branch: repo.branch,
        sparsePath: requestedPath || undefined,
        forceRefresh: request.forceRefresh || undefined,
        goal: `Save ${refLabel(repo)}${requestedPath ? `/${requestedPath}` : ''} locally`,
        reasoning: 'CLI remote-as-local materialization',
      },
    ],
  })) as DirectToolResult;

  if (result.isError) {
    throw new Error(directToolText(result));
  }

  const data = parseCloneResult(result);
  const cloneLocation = data.location;
  if (!cloneLocation?.localPath) {
    throw new Error('ghCloneRepo did not return location.localPath.');
  }

  const repoRoot = path.resolve(cloneLocation.localPath);
  const localPath = requestedPath
    ? path.resolve(repoRoot, ...requestedPath.split('/'))
    : repoRoot;
  const resolvedBranch = cloneLocation.resolvedBranch ?? repo.branch;
  const cached = Boolean(cloneLocation.cached);
  const complete = cloneLocation.complete === true;
  const verified = cloneLocation.verified === true;

  return {
    owner: repo.owner,
    repo: repo.repo,
    location: {
      kind: requestedPath
        ? statSync(localPath).isFile()
          ? 'file'
          : 'directory'
        : 'repo',
      localPath,
      repoRoot,
      ...(requestedPath ? { requestedPath } : {}),
      source: 'clone',
      cached,
      complete,
      verified,
      ...(cloneLocation.commitSha
        ? { commitSha: cloneLocation.commitSha }
        : {}),
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
        goal: `Save ${refLabel(repo)}${requestedPath ? `/${requestedPath}` : ''} locally`,
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
  const dirData = data as FetchDirectoryData;
  const complete = kind === 'file' ? true : dirData.complete === true;
  const verified = dirData.verified ?? false;
  const commitSha = dirData.commitSha;
  const hasSubdirectories = dirData.hasSubdirectories ?? false;
  const skippedSummary = dirData.skippedSummary;

  return {
    owner: repo.owner,
    repo: repo.repo,
    ...(dirData.isPartial ? { isPartial: true } : {}),
    ...(dirData.partialReasons
      ? { partialReasons: dirData.partialReasons }
      : {}),
    ...(dirData.terminalLimit ? { terminalLimit: true } : {}),
    ...(dirData.next ? { next: dirData.next } : {}),
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
