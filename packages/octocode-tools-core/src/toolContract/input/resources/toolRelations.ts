/**
 * Cross-field rules that cannot be represented by a flattened field list.
 * This is the canonical relation source for CLI/MCP schema rendering.
 */
const TOOL_SCHEMA_RELATIONS: Readonly<Record<string, readonly string[]>> = {
  ghSearch: [
    'Use only fields listed for the selected operation.',
    'match: code=file|path; repositories=name|description|readme.',
    "code cannot select branch; it searches GitHub's indexed default branch.",
  ],
  localSearch: [
    'text needs searchText.',
    'structural needs exactly one of pattern or rule.',
    'Fields from different operations cannot be mixed.',
  ],
  localAnalyzeGraph: [
    'dependencies | dependents -> requires file; depth applies.',
    'path -> requires file + target.',
    'deadCode/reachability use entrypoints and includeTests as roots.',
  ],
  localGetFileContent: [
    'Choose fullContent, a line range, or matchString.',
    'A line range needs startLine and endLine.',
    'matchString options apply with matchString.',
  ],
  lspGetSemantics: [
    'workspaceSymbol needs symbolName and may use workspaceRoot.',
    'documentSymbols/diagnostic need uri.',
    'definition | references | hover | callers | callees | callHierarchy | implementation | typeDefinition | supertypes | subtypes -> requires uri + symbolName + lineHint.',
  ],
  ghGetFileContent: [
    'Choose fullContent, a line range, or matchString.',
    'A line range needs startLine and endLine.',
    'matchString options apply with matchString.',
    'type:"directory" materializes; extraction fields read files.',
  ],
  npmSearch: ['Set exactly one of packageName or keywords.'],
};

export function getToolSchemaRelations(toolName: string): string[] {
  return [...(TOOL_SCHEMA_RELATIONS[toolName] ?? [])];
}
