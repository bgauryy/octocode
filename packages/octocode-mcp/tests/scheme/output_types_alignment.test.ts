import { describe, expectTypeOf, it } from 'vitest';

import type {
  GitHubFileContentData,
  GitHubSearchCodeData,
  GitHubSearchCodeGroup,
  GitHubSearchPullRequestsData,
  GitHubPullRequestItem,
  GitHubRepositoryItem,
  GitHubViewRepoStructureData,
  LocalFindFilesEntry,
  LocalSearchCodeFile,
  LocalSearchCodeMatch,
  PackageItem,
  NpmSearchData,
} from '@octocodeai/octocode-core/types';
import type {
  GitHubFetchContentToolResult,
  GitHubDirectoryFileEntry,
  GitHubSearchCodeToolResult,
  GitHubSearchPullRequestsToolResult,
  GitHubSearchRepositoriesData,
  GitHubSearchRepositoriesToolResult,
  GitHubRepositoryOutput,
  GitHubRepoStructureDirectoryEntry,
  LocalGetFileContentToolResult,
  LocalFindFilesToolResult,
  LocalSearchCodeToolResult,
  LocalViewStructureToolResult,
  LocalViewStructureEntryFlat,
} from '@octocodeai/octocode-core/extra-types';
import type {
  ContentResultData,
  ContentResult,
  SearchResult,
  PullRequestInfo,
  PullRequestSearchResultData,
  PullRequestSearchResult,
  SimplifiedRepository,
  RepoSearchResult,
  DirectoryEntry,
  RepoStructureResultData,
  RepoStructureResult,
  FetchContentResult,
  FindFilesResult,
  FoundFile,
  SearchContentResult,
  RipgrepFileMatches,
  RipgrepMatch,
  ViewStructureResult,
  LspGetSemanticsQuery,
  SemanticContentType,
  NpmSearchResult,
  PackageResultWithRepo,
} from '../../src/public.js';

describe('Output type alignment', () => {
  it('derives GitHub tool output types from the output schemas', () => {
    expectTypeOf<ContentResultData>().toEqualTypeOf<GitHubFileContentData>();
    expectTypeOf<ContentResult>().toEqualTypeOf<GitHubFetchContentToolResult>();
    expectTypeOf<GitHubDirectoryFileEntry>().toEqualTypeOf<{
      path: string;
      size: number;
      type: string;
    }>();
    expectTypeOf<ContentResult['results']>().toEqualTypeOf<
      GitHubFileContentData[] | undefined
    >();
    expectTypeOf<ContentResult['status']>().toEqualTypeOf<
      'empty' | 'error' | undefined
    >();

    expectTypeOf<SearchResult>().toEqualTypeOf<GitHubSearchCodeData>();
    expectTypeOf<SearchResult['results']>().toEqualTypeOf<
      readonly GitHubSearchCodeGroup[]
    >();
    expectTypeOf<GitHubSearchCodeToolResult['results']>().toEqualTypeOf<
      GitHubSearchCodeGroup[] | undefined
    >();

    expectTypeOf<PullRequestInfo>().toEqualTypeOf<GitHubPullRequestItem>();
    expectTypeOf<PullRequestSearchResultData>().toEqualTypeOf<GitHubSearchPullRequestsData>();
    expectTypeOf<PullRequestSearchResult>().toEqualTypeOf<GitHubSearchPullRequestsToolResult>();
    expectTypeOf<PullRequestSearchResult['pull_requests']>().toEqualTypeOf<
      GitHubPullRequestItem[] | undefined
    >();

    expectTypeOf<SimplifiedRepository>().toEqualTypeOf<GitHubRepositoryOutput>();
    expectTypeOf<RepoSearchResult>().toEqualTypeOf<GitHubSearchRepositoriesData>();
    expectTypeOf<RepoSearchResult['repositories']>().toEqualTypeOf<
      GitHubRepositoryOutput[]
    >();
    expectTypeOf<
      GitHubSearchRepositoriesToolResult['repositories']
    >().toEqualTypeOf<GitHubRepositoryItem[] | undefined>();

    expectTypeOf<DirectoryEntry>().toEqualTypeOf<GitHubRepoStructureDirectoryEntry>();
    expectTypeOf<RepoStructureResultData>().toEqualTypeOf<GitHubViewRepoStructureData>();
    expectTypeOf<RepoStructureResult['structure']>().toEqualTypeOf<
      Record<string, GitHubRepoStructureDirectoryEntry> | undefined
    >();
  });

  it('derives local tool output types from the output schemas', () => {
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
      LocalSearchCodeMatch[] | undefined
    >();

    expectTypeOf<ViewStructureResult['entries']>().toEqualTypeOf<
      LocalViewStructureEntryFlat[] | undefined
    >();
    expectTypeOf<ViewStructureResult>().toEqualTypeOf<LocalViewStructureToolResult>();
  });

  it('exports current LSP semantic query types', () => {
    expectTypeOf<SemanticContentType>().toEqualTypeOf<
      | 'definition'
      | 'references'
      | 'callers'
      | 'callees'
      | 'callHierarchy'
      | 'hover'
      | 'documentSymbols'
      | 'typeDefinition'
      | 'implementation'
    >();
    expectTypeOf<
      LspGetSemanticsQuery['type']
    >().toEqualTypeOf<SemanticContentType>();
  });

  it('derives package output types from the output schemas', () => {
    expectTypeOf<PackageResultWithRepo>().toEqualTypeOf<PackageItem>();
    expectTypeOf<NpmSearchResult>().toEqualTypeOf<NpmSearchData>();
    expectTypeOf<NpmSearchResult['packages']>().toEqualTypeOf<
      readonly PackageItem[]
    >();
  });
});
