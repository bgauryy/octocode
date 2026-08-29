import type { ToolSpec } from '../../types.js';

export const LOCAL_ANALYZE_GRAPH_TOOL_NAME = 'localAnalyzeGraph' as const;

export const LOCAL_ANALYZE_GRAPH_DESCRIPTION =
  'Analyze the repository file graph with one bounded operation: deadCode, cycles, dependencies, dependents, path, or reachability. Results report syntactic import/re-export provenance and confidence; verify symbol identity and dead-code candidates with lspGetSemantics.';

export const localAnalyzeGraph: ToolSpec = {
  name: LOCAL_ANALYZE_GRAPH_TOOL_NAME,
  type: 'Local',
  shortDescription:
    'Analyze a repository file graph through one bounded operation.',
  instructions:
    'Choose deadCode, cycles, dependencies, dependents, path, or reachability. Use lspGetSemantics to verify symbol identity and every deletion candidate.',
  description: LOCAL_ANALYZE_GRAPH_DESCRIPTION,
  schema: {
    path: 'Absolute repository or package root to scan.',
    operation:
      'One bounded operation: deadCode, cycles, dependencies, dependents, path, or reachability.',
    file: 'Repository-relative source file for dependencies, dependents, or path.',
    target: 'Repository-relative destination file for path.',
    depth: 'Traversal depth for dependencies or dependents.',
    entrypoints: 'Repository-relative roots for deadCode or reachability.',
  },
};
