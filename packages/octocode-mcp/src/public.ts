/**
 * Public API exports for octocode-mcp
 *
 * This module provides types, schemas, and utilities for users
 * who want to programmatically interact with Octocode MCP metadata.
 *
 * @example
 * ```typescript
 * import {
 *   STATIC_TOOL_NAMES,
 *   type CompleteMetadata,
 *   type ToolNames,
 *   type ToolMetadata
 * } from 'octocode-mcp/public';
 * ```
 */

// Server registration and configuration
export * from './public/server.js';

// Tool execution functions and security
export * from './public/tools.js';

// Zod schemas (re-exported from @octocodeai/octocode-core for convenience)
export {
  GitHubCodeSearchQuerySchema,
  GitHubViewRepoStructureQuerySchema,
  GitHubReposSearchSingleQuerySchema,
  GitHubPullRequestSearchQuerySchema,
  FileContentQuerySchema,
  RipgrepQuerySchema,
  FetchContentQuerySchema,
  FindFilesQuerySchema,
  ViewStructureQuerySchema,
  LSPGotoDefinitionQuerySchema,
  LSPFindReferencesQuerySchema,
  LSPCallHierarchyQuerySchema,
  PackageSearchQuerySchema,
  CloneRepoQuerySchema,
} from '@octocodeai/octocode-core';

// Tool metadata loading
export { loadToolContent } from './tools/toolMetadata/state.js';

// Response formatting
export * from './public/responses.js';

// Session management
export * from './public/session.js';
