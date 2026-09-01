export interface ToolSchemaVariant {
  name: string;
  when: string;
  requires: string[];
  fields?: string[];
  excludes?: string[];
  example: Record<string, unknown>;
}

const variants: Record<string, ToolSchemaVariant[]> = {
  ghSearch: [
    {
      name: 'code',
      when: 'Search code contents or file paths',
      requires: ['operation'],
      excludes: ['branch'],
      fields: [
        'keywords',
        'owner',
        'repo',
        'extension',
        'filename',
        'path',
        'language',
        'match',
        'pageSize',
        'page',
        'concise',
      ],
      example: {
        operation: 'code',
        keywords: ['toolSchemas'],
        owner: 'bgauryy',
        repo: 'octocode',
      },
    },
    {
      name: 'repositories',
      when: 'Discover repositories',
      requires: ['operation'],
      fields: [
        'keywords',
        'topics',
        'language',
        'owner',
        'stars',
        'forks',
        'goodFirstIssues',
        'updated',
        'created',
        'size',
        'match',
        'sort',
        'pageSize',
        'page',
        'archived',
        'visibility',
        'license',
        'concise',
      ],
      example: { operation: 'repositories', keywords: ['octocode'] },
    },
    {
      name: 'tree',
      when: 'Browse a known repository tree',
      requires: ['operation', 'owner', 'repo'],
      fields: [
        'owner',
        'repo',
        'branch',
        'path',
        'maxDepth',
        'page',
        'pageSize',
        'include',
      ],
      example: {
        operation: 'tree',
        owner: 'bgauryy',
        repo: 'octocode',
        path: 'packages',
        maxDepth: 2,
      },
    },
  ],
  localSearch: [
    {
      name: 'text',
      when: 'Search lexical text or regular expressions',
      requires: ['operation', 'path', 'searchText'],
      example: { operation: 'text', path: '.', searchText: 'toolSchemas' },
    },
    {
      name: 'structural',
      when: 'Match AST structure',
      requires: ['operation', 'path', 'pattern'],
      example: {
        operation: 'structural',
        path: '.',
        pattern: 'console.log($A)',
        langType: 'typescript',
      },
    },
    {
      name: 'files',
      when: 'Discover paths by name or metadata',
      requires: ['operation', 'path'],
      example: { operation: 'files', path: '.', names: ['*.ts'] },
    },
    {
      name: 'tree',
      when: 'Browse a directory tree',
      requires: ['operation', 'path'],
      example: { operation: 'tree', path: '.', maxDepth: 5 },
    },
  ],
  localAnalyzeGraph: [
    {
      name: 'dependencies',
      when: 'Find files imported by a file',
      requires: ['path', 'operation', 'file'],
      example: { path: '.', operation: 'dependencies', file: 'src/index.ts' },
    },
    {
      name: 'dependents',
      when: 'Find files importing a file',
      requires: ['path', 'operation', 'file'],
      example: { path: '.', operation: 'dependents', file: 'src/index.ts' },
    },
    {
      name: 'path',
      when: 'Find a shortest dependency path',
      requires: ['path', 'operation', 'file', 'target'],
      example: {
        path: '.',
        operation: 'path',
        file: 'src/a.ts',
        target: 'src/b.ts',
      },
    },
    {
      name: 'cycles',
      when: 'Find dependency cycles',
      requires: ['path', 'operation'],
      example: { path: '.', operation: 'cycles' },
    },
    {
      name: 'deadCode',
      when: 'Find unreachable file candidates',
      requires: ['path', 'operation'],
      example: { path: '.', operation: 'deadCode' },
    },
    {
      name: 'reachability',
      when: 'Find files unreachable from selected roots',
      requires: ['path', 'operation', 'entrypoints'],
      example: {
        path: '.',
        operation: 'reachability',
        entrypoints: ['src/index.ts'],
      },
    },
  ],
  lspGetSemantics: [
    {
      name: 'anchored',
      when: 'Query a named symbol near a source line',
      requires: ['uri', 'type', 'symbolName', 'lineHint'],
      example: {
        uri: 'src/index.ts',
        type: 'definition',
        symbolName: 'main',
        lineHint: 1,
      },
    },
    {
      name: 'document',
      when: 'Query a whole document',
      requires: ['uri', 'type'],
      example: { uri: 'src/index.ts', type: 'documentSymbols' },
    },
    {
      name: 'workspace',
      when: 'Search symbols across the workspace',
      requires: ['workspaceRoot', 'type', 'symbolName'],
      example: {
        workspaceRoot: '.',
        type: 'workspaceSymbol',
        symbolName: 'Schema',
      },
    },
  ],
};

export function getToolSchemaVariants(name: string): ToolSchemaVariant[] {
  return variants[name] ?? [];
}
