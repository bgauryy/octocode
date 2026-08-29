export interface ToolNames {
  GITHUB_SEARCH_CODE: 'ghSearchCode';
  GITHUB_FETCH_CONTENT: 'ghGetFileContent';
  GITHUB_VIEW_REPO_STRUCTURE: 'ghViewRepoStructure';
  GITHUB_SEARCH_REPOSITORIES: 'ghSearchRepos';
  GITHUB_PULL_REQUESTS: 'ghSearchPullRequests';
  GITHUB_ISSUES: 'ghSearchIssues';
  GITHUB_COMMITS: 'ghSearchCommits';
  GITHUB_RELEASES: 'ghListReleases';
  GITHUB_DISCUSSIONS: 'ghSearchDiscussions';
  PACKAGE_SEARCH: 'npmSearch';
  GITHUB_CLONE_REPO: 'ghCloneRepo';
  LOCAL_RIPGREP: 'localSearchCode';
  LOCAL_VIEW_STRUCTURE: 'localViewStructure';
  LOCAL_FIND_FILES: 'localFindFiles';
  LOCAL_ANALYZE_GRAPH: 'localAnalyzeGraph';
  LOCAL_FETCH_CONTENT: 'localGetFileContent';
  LSP_GET_SEMANTIC_CONTENT: 'lspGetSemantics';
}

export interface ToolSchema {
  readonly [param: string]: string;
}

export type ToolType = 'Github' | 'Local' | 'NPM';

export interface ToolSpec {
  readonly name: string;
  readonly type: ToolType;
  readonly shortDescription: string;
  readonly instructions: string;
  readonly description: string;
  readonly schema: ToolSchema;
}

export interface BaseSchemaDescriptions {
  readonly id: string;
  readonly mainResearchGoal: string;
  readonly researchGoal: string;
  readonly reasoning: string;
}

export interface LocalCompleteMetadata {
  readonly systemPrompt: string;
  readonly toolNames: ToolNames;
  readonly baseSchema: BaseSchemaDescriptions;
  readonly tools: Record<string, ToolSpec>;
}
