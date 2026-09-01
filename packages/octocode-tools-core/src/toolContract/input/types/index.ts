export interface ToolNames {
  GITHUB_SEARCH: 'ghSearch';
  GITHUB_FETCH_CONTENT: 'ghGetFileContent';
  GITHUB_SEARCH_HISTORY: 'ghSearchHistory';
  GITHUB_GET_HISTORY_ITEM: 'ghGetHistoryItem';
  PACKAGE_SEARCH: 'npmSearch';
  GITHUB_CLONE_REPO: 'ghCloneRepo';
  LOCAL_SEARCH: 'localSearch';
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
