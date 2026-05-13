/**
 * Security-related constants for the MCP server
 */

/**
 * Allowed Linux commands (whitelist)
 * Only commands actually used by the active tools:
 * - rg: Ripgrep for fast pattern searching (used by localSearchCode)
 * - ls: List directory contents (used by localViewStructure)
 * - find: Search for files and directories (used by localFindFiles)
 * - grep: Pattern search fallback (used by lspFindReferences pattern matching)
 */
export const ALLOWED_COMMANDS = [
  'rg', // Ripgrep - Fast pattern search (localSearchCode tool)
  'ls', // List directory contents (localViewStructure tool)
  'find', // Find files/directories recursively (localFindFiles tool)
  'grep', // Pattern search fallback (lspFindReferences pattern matching)
  'git', // Git - Shallow clone repositories (githubCloneRepo tool)
] as const;

/**
 * Dangerous shell metacharacters for command injection prevention.
 * These patterns are checked for non-pattern arguments (paths, filenames).
 */
export const DANGEROUS_PATTERNS = [
  /[;&|`$(){}[\]<>]/, // Shell metacharacters
  /\${/, // Variable expansion
  /\$\(/, // Command substitution
] as const;

/**
 * Dangerous patterns specific to search/glob patterns.
 * These are more permissive than DANGEROUS_PATTERNS - they allow regex chars
 * like [], (), |, {} that are legitimate in search patterns, but still block
 * shell injection vectors like command substitution and variable expansion.
 */
export const PATTERN_DANGEROUS_PATTERNS = [
  /\${/, // Variable expansion
  /\$\(/, // Command substitution
  /`/, // Backtick substitution
  /;/, // Command separator
] as const;
