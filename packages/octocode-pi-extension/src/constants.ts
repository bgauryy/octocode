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

export const OCTOCODE_SUPPORT_TOOL_NAMES = [
  'web',
  'chromeDebug',
  'browserAgent',
  'spawnSubagent',
  'manage_context',
  'spawnAgent',
  'AgentMessage',
  'memory_recall',
  'memory_record',
  'memory_reflect',
  'workspace_status',
  'memory_workspace_status',
  'agent_signal',
  'file_lock',
  'memory_file_lock',
  'memory_notify',
  'memory_refine_get',
  'memory_audit_unverified',
  'memory_verify',
  'memory_export_harness',
] as const;
