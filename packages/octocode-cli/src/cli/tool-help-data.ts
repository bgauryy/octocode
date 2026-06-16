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
    name: 'ghSearchCode',
    category: 'GitHub',
    fields: '[keywordsToSearch*, owner?, repo?]',
  },
  {
    name: 'ghGetFileContent',
    category: 'GitHub',
    fields: '[owner*, repo*, path?]',
  },
  {
    name: 'ghViewRepoStructure',
    category: 'GitHub',
    fields: '[owner*, repo*, path?]',
  },
  {
    name: 'ghSearchRepos',
    category: 'GitHub',
    fields: '[keywordsToSearch?, owner?, language?]',
  },
  {
    name: 'ghSearchPRs',
    category: 'GitHub',
    fields: '[owner*, repo?, prNumber?]',
  },
  {
    name: 'ghCloneRepo',
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
    fields: '[path*, names?, regex?, pathPattern?]',
  },
  {
    name: 'localGetFileContent',
    category: 'Local',
    fields: '[path*, matchString?, startLine?]',
  },
  {
    name: 'lspGetSemantics',
    category: 'LSP',
    fields: '[uri*, type, symbolName?, lineHint?]',
  },
  {
    name: 'npmSearch',
    category: 'Package',
    fields: '[packageName*]',
  },
];
