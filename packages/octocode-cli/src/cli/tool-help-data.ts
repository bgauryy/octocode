export type HelpToolDefinition = {
  readonly name: string;
  readonly category: string;
  readonly fields: string;
};

export const HELP_TOOL_CATEGORIES = [
  'GitHub',
  'Local',
  'LSP',
  'Package',
] as const;

export const HELP_TOOL_DEFINITIONS: readonly HelpToolDefinition[] = [
  {
    name: 'githubSearchCode',
    category: 'GitHub',
    fields: '[keywordsToSearch*, owner?, repo?]',
  },
  {
    name: 'githubGetFileContent',
    category: 'GitHub',
    fields: '[owner*, repo*, path?]',
  },
  {
    name: 'githubViewRepoStructure',
    category: 'GitHub',
    fields: '[owner*, repo*, path?]',
  },
  {
    name: 'githubSearchRepositories',
    category: 'GitHub',
    fields: '[keywordsToSearch?, owner?, language?]',
  },
  {
    name: 'githubSearchPullRequests',
    category: 'GitHub',
    fields: '[owner*, repo?, prNumber?]',
  },
  {
    name: 'githubCloneRepo',
    category: 'GitHub',
    fields: '[owner*, repo*, branch?]',
  },
  {
    name: 'localSearchCode',
    category: 'Local',
    fields: '[path*, keywords*, include?]',
  },
  {
    name: 'localViewStructure',
    category: 'Local',
    fields: '[path*, depth?]',
  },
  {
    name: 'localFindFiles',
    category: 'Local',
    fields: '[path*, pattern?, extension?]',
  },
  {
    name: 'localGetFileContent',
    category: 'Local',
    fields: '[path*, matchString?, startLine?]',
  },
  {
    name: 'lspGetSemanticContent',
    category: 'LSP',
    fields: '[uri*, type, symbolName?, lineHint?]',
  },
  {
    name: 'packageSearch',
    category: 'Package',
    fields: '[packageName?, keywords?]',
  },
];
