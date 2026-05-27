import { describe, expectTypeOf, it } from 'vitest';

import type {
  GitHubFetchContentData,
  GitHubFetchContentToolResult,
  GitHubDirectoryFileEntry,
  GitHubSearchCodeData,
  GitHubSearchCodeToolResult,
  GitHubCodeSearchFile,
  GitHubSearchPullRequestsData,
  GitHubSearchPullRequestsToolResult,
  GitHubPullRequestOutput,
  GitHubSearchRepositoriesData,
  GitHubSearchRepositoriesToolResult,
  GitHubRepositoryOutput,
  GitHubViewRepoStructureData,
  GitHubRepoStructureDirectoryEntry,
  LocalGetFileContentToolResult,
  LocalFindFilesToolResult,
  LocalFindFilesEntry,
  LocalSearchCodeToolResult,
  LocalSearchCodeFile,
  LocalSearchCodeMatch,
  LocalViewStructureToolResult,
  LocalViewStructureEntry,
  LspGotoDefinitionToolResult,
  LspCodeSnippet,
  LspFindReferencesToolResult,
  LspReferenceLocation,
  LspCallHierarchyToolResult,
  LspCallHierarchyItem,
  LspIncomingCall,
  LspOutgoingCall,
  PackageSearchData,
  PackageSearchPackage,
} from '@octocodeai/octocode-core';
import type {
  ContentResultData,
  ContentResult,
  DirectoryFileEntry,
  SearchResult,
  PullRequestInfo,
  PullRequestSearchResultData,
  PullRequestSearchResult,
  SimplifiedRepository,
  RepoSearchResult,
  DirectoryEntry,
  RepoStructureResultData,
  FetchContentResult,
  FindFilesResult,
  FoundFile,
  SearchContentResult,
  RipgrepFileMatches,
  RipgrepMatch,
  ViewStructureResult,
  GotoDefinitionResult,
  CodeSnippet,
  FindReferencesResult,
  ReferenceLocation,
  CallHierarchyResult,
  CallHierarchyItem,
  IncomingCall,
  OutgoingCall,
  PackageSearchResult,
  PackageResultWithRepo,
} from '../../src/public.js';

describe('Output type alignment', () => {
  it('derives GitHub tool output aliases from the output schemas', () => {
    expectTypeOf<ContentResultData>().toEqualTypeOf<GitHubFetchContentData>();
    expectTypeOf<ContentResult>().toEqualTypeOf<GitHubFetchContentToolResult>();
    expectTypeOf<DirectoryFileEntry>().toEqualTypeOf<GitHubDirectoryFileEntry>();
    expectTypeOf<ContentResult['content']>().toEqualTypeOf<
      string | undefined
    >();
    expectTypeOf<ContentResult['status']>().toEqualTypeOf<
      'hasResults' | 'empty' | 'error'
    >();

    expectTypeOf<SearchResult>().toEqualTypeOf<GitHubSearchCodeData>();
    expectTypeOf<SearchResult['files']>().toEqualTypeOf<
      GitHubCodeSearchFile[] | undefined
    >();
    expectTypeOf<GitHubSearchCodeToolResult['files']>().toEqualTypeOf<
      GitHubCodeSearchFile[] | undefined
    >();

    expectTypeOf<PullRequestInfo>().toEqualTypeOf<GitHubPullRequestOutput>();
    expectTypeOf<PullRequestSearchResultData>().toEqualTypeOf<GitHubSearchPullRequestsData>();
    expectTypeOf<PullRequestSearchResult>().toEqualTypeOf<GitHubSearchPullRequestsToolResult>();
    expectTypeOf<PullRequestSearchResult['pull_requests']>().toEqualTypeOf<
      GitHubPullRequestOutput[] | undefined
    >();

    expectTypeOf<SimplifiedRepository>().toEqualTypeOf<GitHubRepositoryOutput>();
    expectTypeOf<RepoSearchResult>().toEqualTypeOf<GitHubSearchRepositoriesData>();
    expectTypeOf<
      GitHubSearchRepositoriesToolResult['repositories']
    >().toEqualTypeOf<GitHubRepositoryOutput[] | undefined>();

    expectTypeOf<DirectoryEntry>().toEqualTypeOf<GitHubRepoStructureDirectoryEntry>();
    expectTypeOf<RepoStructureResultData>().toEqualTypeOf<GitHubViewRepoStructureData>();
    expectTypeOf<GitHubViewRepoStructureData['structure']>().toEqualTypeOf<
      Record<string, GitHubRepoStructureDirectoryEntry> | undefined
    >();
  });

  it('derives local tool output aliases from the output schemas', () => {
    expectTypeOf<FetchContentResult>().toEqualTypeOf<LocalGetFileContentToolResult>();

    expectTypeOf<FoundFile>().toEqualTypeOf<LocalFindFilesEntry>();
    expectTypeOf<FindFilesResult>().toEqualTypeOf<LocalFindFilesToolResult>();
    expectTypeOf<FindFilesResult['files']>().toEqualTypeOf<
      LocalFindFilesEntry[] | undefined
    >();

    expectTypeOf<RipgrepMatch>().toEqualTypeOf<LocalSearchCodeMatch>();
    expectTypeOf<RipgrepFileMatches>().toEqualTypeOf<LocalSearchCodeFile>();
    expectTypeOf<SearchContentResult>().toEqualTypeOf<LocalSearchCodeToolResult>();
    expectTypeOf<SearchContentResult['files']>().toEqualTypeOf<
      LocalSearchCodeFile[] | undefined
    >();
    expectTypeOf<RipgrepFileMatches['matches']>().toEqualTypeOf<
      LocalSearchCodeMatch[]
    >();

    expectTypeOf<ViewStructureResult['entries']>().toEqualTypeOf<
      LocalViewStructureEntry[] | undefined
    >();
    expectTypeOf<ViewStructureResult>().toEqualTypeOf<LocalViewStructureToolResult>();
  });

  it('derives LSP and package output aliases from the output schemas', () => {
    expectTypeOf<CodeSnippet>().toEqualTypeOf<LspCodeSnippet>();
    expectTypeOf<GotoDefinitionResult>().toEqualTypeOf<LspGotoDefinitionToolResult>();
    expectTypeOf<GotoDefinitionResult['locations']>().toEqualTypeOf<
      LspCodeSnippet[] | undefined
    >();

    expectTypeOf<ReferenceLocation>().toEqualTypeOf<LspReferenceLocation>();
    expectTypeOf<FindReferencesResult>().toEqualTypeOf<LspFindReferencesToolResult>();
    expectTypeOf<FindReferencesResult['locations']>().toEqualTypeOf<
      LspReferenceLocation[] | undefined
    >();

    expectTypeOf<CallHierarchyItem>().toEqualTypeOf<LspCallHierarchyItem>();
    expectTypeOf<IncomingCall>().toEqualTypeOf<LspIncomingCall>();
    expectTypeOf<OutgoingCall>().toEqualTypeOf<LspOutgoingCall>();
    expectTypeOf<CallHierarchyResult>().toEqualTypeOf<LspCallHierarchyToolResult>();
    expectTypeOf<CallHierarchyResult['incomingCalls']>().toEqualTypeOf<
      LspIncomingCall[] | undefined
    >();

    expectTypeOf<PackageResultWithRepo>().toEqualTypeOf<PackageSearchPackage>();
    expectTypeOf<PackageSearchResult>().toEqualTypeOf<PackageSearchData>();
    expectTypeOf<PackageSearchResult['packages']>().toEqualTypeOf<
      PackageSearchPackage[]
    >();
  });
});
