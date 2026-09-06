/**
 * Pi's config directory name. Mirrors the host's exported CONFIG_DIR_NAME /
 * package.json `piConfig.configDir`; kept as a local constant so the extension
 * never hard-imports the host barrel at runtime. A contract test pins this
 * against the installed pi package, so a host rename fails loudly here instead
 * of silently writing to the wrong directory.
 */
export const PI_CONFIG_DIR = '.pi';
export const SYSTEM_PROMPT_MARKER = '<!-- octocode-pi-extension:system-prompt -->';
export const MANAGED_BLOCK_START = '<!-- OCTOCODE_PI_EXTENSION_APPEND_SYSTEM_START -->';
export const MANAGED_BLOCK_END = '<!-- OCTOCODE_PI_EXTENSION_APPEND_SYSTEM_END -->';

// Research tools (GitHub, local, LSP, npm) are served via MCPTool → octocode MCP server.
// They are NOT registered as native Pi tools. See mcp-tool.ts DEFAULT_OCTOCODE_MCP_SERVER.

// Replaced by Octocode MCPTool-backed equivalents: localGetFileContent and localSearch operations.
export const DISABLED_BUILTIN_TOOL_NAMES = ['read', 'edit', 'write', 'grep', 'find', 'ls'] as const;

// Same-name registerTool overrides (Pi keeps the name; Octocode owns the implementation).
export const OVERRIDDEN_BUILTIN_TOOL_NAMES = ['bash'] as const;

const COMMON_SUPPORT_TOOL_NAMES = [
  'file',
  'web',
  'chromeDebug',
  'agent',
  'callTool',
  'skill',
  'plan',
  'localServer',
  'MCPTool',
  'askUser',
  'memory',
] as const;

const MEDIA_SUPPORT_TOOL_NAMES = ['readMedia', 'media', 'runFfmpeg'] as const;

// Default model-callable support tools. Together with the overridden bash tool,
// this is the 17-tool unified direct palette. MCP research tools and
// slash commands are separate surfaces and are not counted here.
export const OCTOCODE_SUPPORT_TOOL_NAMES = [
  ...COMMON_SUPPORT_TOOL_NAMES,
  'lock',
  'message',
  ...MEDIA_SUPPORT_TOOL_NAMES,
] as const;

/**
 * Heuristic characters-per-token divisor for the prompt-budget / footer overhead
 * estimates. Single source so the `/octocode-status` budget and the toolbar Σ
 * segment always agree. ~4 is the common English-text approximation.
 */
export const CHARS_PER_TOKEN = 4 as const;
