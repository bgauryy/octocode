export const PACKAGE_NAME = '@octocodeai/pi-extension';
export const SYSTEM_PROMPT_MARKER = '<!-- octocode-pi-extension:system-prompt -->';
export const MANAGED_BLOCK_START = '<!-- OCTOCODE_PI_EXTENSION_APPEND_SYSTEM_START -->';
export const MANAGED_BLOCK_END = '<!-- OCTOCODE_PI_EXTENSION_APPEND_SYSTEM_END -->';

export const OCTOCODE_DIRECT_TOOL_NAMES = [
  'ghSearchCode',
  'ghSearchRepos',
  'ghHistoryResearch',
  'ghGetFileContent',
  'ghViewRepoStructure',
  'ghCloneRepo',
  'localSearchCode',
  'localFindFiles',
  'localGetFileContent',
  'localViewStructure',
  'lspGetSemantics',
  'localBinaryInspect',
  'npmSearch',
] as const;

// Replaced by superior Octocode tools: localGetFileContent, localSearchCode, localFindFiles, localViewStructure
export const DISABLED_BUILTIN_TOOL_NAMES = ['read', 'grep', 'find', 'ls'] as const;

// Same-name registerTool overrides (Pi keeps the name; Octocode owns the implementation).
export const OVERRIDDEN_BUILTIN_TOOL_NAMES = ['edit', 'write', 'bash'] as const;

// Awareness memory/coordination is intentionally absent: it is not exposed as
// agent tools. Agents drive it through the octocode-awareness CLI
// (node $OCTOCODE_AWARENESS_CLI <noun> <verb>) and the octocode-awareness skill,
// with the edit/verify lifecycle automated by the awareness hooks.
export const OCTOCODE_SUPPORT_TOOL_NAMES = [
  'web',
  'chromeDebug',
  'browserAgent',
  'spawnSubagent',
  'manage_context',
  'spawnAgent',
  'AgentMessage',
] as const;
