/**
 * Tools Info Routes - Expose tool metadata via HTTP
 *
 * Routes:
 *   GET  /tools/list            - List all tools (concise)
 *   GET  /tools/info            - List all tools with details
 *   GET  /tools/info/:toolName  - Get specific tool info
 *   POST /tools/call/:toolName  - Execute a tool directly (simplified!)
 *
 * Query params:
 *   schema=true  - Include schema information
 *   hints=true   - Include hints information
 */

import { Router, type Request, type Response, type NextFunction } from 'express';
import { getMcpContent } from '../mcpCache.js';
import { transformToJsonSchema } from '../types/mcp.js';
import { z } from 'zod';

// Import Zod schemas from octocode-mcp (source of truth)
import {
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
} from 'octocode-mcp/public';
import {
  githubSearchCode,
  githubGetFileContent,
  githubViewRepoStructure,
  githubSearchRepositories,
  githubSearchPullRequests,
  packageSearch,
  localSearchCode,
  localGetFileContent,
  localFindFiles,
  localViewStructure,
  lspGotoDefinition,
  lspFindReferences,
  lspCallHierarchy,
  logToolCall,
} from '../index.js';
import {
  withGitHubResilience,
  withLocalResilience,
  withLspResilience,
  withPackageResilience,
} from '../utils/resilience.js';
import { parseToolResponse, parseToolResponseBulk } from '../utils/responseParser.js';
import { fireAndForgetWithTimeout } from '../utils/asyncTimeout.js';
import { validateToolCallBody, getValidationHints } from '../validation/toolCallSchema.js';
import { checkReadiness } from '../middleware/readiness.js';

export const toolsRoutes = Router();

// Apply readiness check middleware to all tools routes
toolsRoutes.use(checkReadiness);

declare const __PACKAGE_VERSION__: string;
const PACKAGE_VERSION = __PACKAGE_VERSION__;

interface ToolsInfoQuery {
  schema?: string;
  hints?: string;
}

// ============================================================================
// Zod Schema Registry - Maps tool names to their Zod schemas (source of truth)
// ============================================================================

const TOOL_ZOD_SCHEMAS: Record<string, z.ZodType> = {
  // GitHub tools
  githubSearchCode: GitHubCodeSearchQuerySchema,
  githubGetFileContent: FileContentQuerySchema,
  githubViewRepoStructure: GitHubViewRepoStructureQuerySchema,
  githubSearchRepositories: GitHubReposSearchSingleQuerySchema,
  githubSearchPullRequests: GitHubPullRequestSearchQuerySchema,
  // Local tools
  localSearchCode: RipgrepQuerySchema,
  localGetFileContent: FetchContentQuerySchema,
  localFindFiles: FindFilesQuerySchema,
  localViewStructure: ViewStructureQuerySchema,
  // LSP tools
  lspGotoDefinition: LSPGotoDefinitionQuerySchema,
  lspFindReferences: LSPFindReferencesQuerySchema,
  lspCallHierarchy: LSPCallHierarchyQuerySchema,
  // Package tools
  packageSearch: PackageSearchQuerySchema,
};

function toJsonSchema(schema: z.ZodType): Record<string, unknown> | null {
  try {
    return z.toJSONSchema(schema) as Record<string, unknown>;
  } catch {
    return null;
  }
}

function getToolJsonSchema(toolName: string): Record<string, unknown> | null {
  const zodSchema = TOOL_ZOD_SCHEMAS[toolName];
  return zodSchema ? toJsonSchema(zodSchema) : null;
}

/**
 * GET /tools/list - Static list of all tools (concise discovery)
 *
 * Returns static JSON with tool names and short descriptions.
 * Use /tools/info/:toolName to get full description + schema BEFORE calling a tool.
 */
toolsRoutes.get('/list', (_req: Request, res: Response) => {
  res.json({
    success: true,
    data: {
      tools: [
        { name: 'githubSearchCode', description: 'Search code in GitHub repos' },
        { name: 'githubGetFileContent', description: 'Read file from GitHub repo' },
        { name: 'githubViewRepoStructure', description: 'View GitHub repo tree' },
        { name: 'githubSearchRepositories', description: 'Search GitHub repositories' },
        { name: 'githubSearchPullRequests', description: 'Search pull requests' },
        { name: 'packageSearch', description: 'Search npm/PyPI packages' },
        { name: 'localSearchCode', description: 'Search local code with ripgrep' },
        { name: 'localGetFileContent', description: 'Read local file content' },
        { name: 'localFindFiles', description: 'Find files by pattern/metadata' },
        { name: 'localViewStructure', description: 'View local directory tree' },
        { name: 'lspGotoDefinition', description: 'Go to symbol definition' },
        { name: 'lspFindReferences', description: 'Find all symbol references' },
        { name: 'lspCallHierarchy', description: 'Get call hierarchy' },
      ],
    },
    hints: ['GET /tools/info/{name} for full schema before calling'],
  });
});

/**
 * GET /tools/info - Get info about all available tools
 */
toolsRoutes.get('/info', async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const content = getMcpContent();
    
    const query = req.query as ToolsInfoQuery;
    const includeSchema = query.schema === 'true';
    const includeHints = query.hints === 'true';
    
    const toolNames = Object.keys(content.tools);
    const tools = toolNames.map(name => {
      const tool = content.tools[name];
      const result: Record<string, unknown> = {
        name: tool.name,
        description: tool.description,
      };
      
      if (includeSchema) {
        result.schema = tool.schema;
      }
      
      if (includeHints) {
        result.hints = {
          hasResults: tool.hints.hasResults,
          empty: tool.hints.empty,
        };
      }
      
      return result;
    });
    
    const response: Record<string, unknown> = {
      totalTools: toolNames.length,
      toolNames,
      tools,
    };

    if (includeHints) {
      response.baseHints = content.baseHints;
      response.genericErrorHints = content.genericErrorHints;
    }

    res.json({
      success: true,
      data: response,
      hints: ['Use /tools/info/:{{TOOL_NAME}} to get the scheme and description before using it'],
    });
  } catch (error) {
    next(error);
  }
});

/**
 * GET /tools/info/:toolName - Get full info for a specific tool (call before using!)
 *
 * Returns complete description, JSON schema, and hints for a tool.
 * ALWAYS call this before using a tool to understand its parameters.
 *
 * @example
 * GET /tools/info/localSearchCode
 *
 * Response includes:
 * - name: Tool name
 * - description: Full description with usage details
 * - schema: Complete JSON schema with all parameters
 * - hints: Success/empty result hints
 */
toolsRoutes.get('/info/:toolName', async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const content = getMcpContent();

    const { toolName } = req.params;
    const query = req.query as ToolsInfoQuery;
    const includeSchema = query.schema !== 'false';  // Default true for specific tool
    const includeHints = query.hints !== 'false';    // Default true for specific tool

    const tool = content.tools[toolName];

    if (!tool) {
      const availableTools = Object.keys(content.tools);
      res.status(404).json({
        success: false,
        data: null,
        hints: [
          `Tool not found: ${toolName}`,
          `Available tools: ${availableTools.slice(0, 5).join(', ')}...`,
          'Check spelling or use /tools/list to see all tools',
        ],
      });
      return;
    }

    const result: Record<string, unknown> = {
      name: tool.name,
      description: tool.description,
    };

    if (includeSchema) {
      // Try to get the actual Zod schema (source of truth with correct types and required fields)
      const zodJsonSchema = getToolJsonSchema(toolName);
      if (zodJsonSchema) {
        result.inputSchema = zodJsonSchema;
        result._schemaSource = 'zod'; // Indicates this is from the authoritative Zod schema
      } else {
        // Fallback to metadata-based schema (less accurate)
        result.inputSchema = transformToJsonSchema(tool.schema, tool.name);
        result._schemaSource = 'metadata'; // Indicates this is a fallback
      }
    }

    if (includeHints) {
      result.toolHints = {
        hasResults: tool.hints.hasResults,
        empty: tool.hints.empty,
      };
    }

    res.json({
      success: true,
      data: result,
      hints: ['Review schema carefully before calling this tool'],
    });
  } catch (error) {
    next(error);
  }
});

/**
 * GET /tools/metadata - Get raw complete metadata (advanced)
 */
toolsRoutes.get('/metadata', async (
  _req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const content = getMcpContent();

    res.json({
      success: true,
      data: {
        instructions: content.instructions,
        toolCount: Object.keys(content.tools).length,
        promptCount: Object.keys(content.prompts).length,
        hasBaseSchema: !!content.baseSchema,
      },
      hints: ['Use /tools/info for detailed tool information'],
    });
  } catch (error) {
    next(error);
  }
});

/**
 * GET /tools/schemas - Get all tools with their complete JSON schemas
 *
 * Returns all tool names with their full JSON schemas (from Zod).
 * Useful for bulk schema retrieval without calling /info/:toolName for each.
 *
 * @example
 * GET /tools/schemas
 *
 * Response:
 * {
 *   "success": true,
 *   "data": {
 *     "totalTools": 13,
 *     "schemas": {
 *       "localSearchCode": { "type": "object", "properties": {...} },
 *       "githubSearchCode": { "type": "object", "properties": {...} },
 *       ...
 *     }
 *   }
 * }
 */
toolsRoutes.get('/schemas', async (
  _req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const schemas: Record<string, Record<string, unknown>> = {};
    const toolNames = Object.keys(TOOL_ZOD_SCHEMAS);
    
    for (const toolName of toolNames) {
      const schema = getToolJsonSchema(toolName);
      if (schema) {
        schemas[toolName] = schema;
      }
    }
    
    res.json({
      success: true,
      data: {
        totalTools: toolNames.length,
        schemas,
      },
      hints: ['All schemas derived from Zod (source of truth)'],
    });
  } catch (error) {
    next(error);
  }
});

/**
 * GET /tools/system - Get the FULL system instructions
 *
 * Returns the complete system prompt that should be loaded into context FIRST.
 * This defines the agent's behavior, methodology, and best practices.
 *
 * @example
 * GET /tools/system
 *
 * Response:
 * {
 *   "instructions": "## Expert Code Forensics Agent...",
 *   "_meta": { "charCount": 5432, "version": "2.2.0" }
 * }
 */
toolsRoutes.get('/system', async (
  _req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const content = getMcpContent();

    res.json({
      success: true,
      data: {
        instructions: content.instructions,
        charCount: content.instructions.length,
        version: PACKAGE_VERSION,
      },
      hints: ['Load this system prompt FIRST before using tools'],
    });
  } catch (error) {
    next(error);
  }
});

/**
 * GET /tools/initContext - Combined system prompt and all tool schemas
 *
 * Combines /tools/system + /tools/schemas in one call for faster init.
 * Returns:
 * - system_prompt: Full instructions
 * - tools_schema: All tool JSON schemas from Zod
 *
 * @example
 * GET /tools/initContext
 *
 * Response:
 * {
 *   "success": true,
 *   "system_prompt": "## Expert Code Forensics Agent...",
 *   "tools_schema": { "localSearchCode": {...}, ... },
 *   "_meta": { "promptCharCount": 5432, "toolsCount": 13, "version": "2.2.0" }
 * }
 */
toolsRoutes.get('/initContext', async (
  _req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const content = getMcpContent();

    // Build schemas (same as /tools/schemas)
    const schemas: Record<string, Record<string, unknown>> = {};
    for (const toolName of Object.keys(TOOL_ZOD_SCHEMAS)) {
      const schema = getToolJsonSchema(toolName);
      if (schema) schemas[toolName] = schema;
    }

    res.json({
      success: true,
      system_prompt: content.instructions,
      tools_schema: schemas,
      _meta: {
        promptCharCount: content.instructions.length,
        toolsCount: Object.keys(schemas).length,
        version: PACKAGE_VERSION,
      },
    });
  } catch (error) {
    next(error);
  }
});

// ============================================================================
// Tool Registry - Maps tool names to functions and resilience wrappers
// ============================================================================

type ResilienceFn = <T>(fn: () => Promise<T>, toolName: string) => Promise<T>;

/**
 * Tool registry entry for dynamic dispatch.
 * Tool functions are dispatched by name at runtime; Zod schemas validate inputs.
 */
interface ToolEntry {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  fn: (params: any) => Promise<any>;
  resilience: ResilienceFn;
  category: 'github' | 'local' | 'lsp' | 'package';
}

const TOOL_REGISTRY: Record<string, ToolEntry> = {
  // GitHub tools
  githubSearchCode: { fn: githubSearchCode, resilience: withGitHubResilience, category: 'github' },
  githubGetFileContent: { fn: githubGetFileContent, resilience: withGitHubResilience, category: 'github' },
  githubViewRepoStructure: { fn: githubViewRepoStructure, resilience: withGitHubResilience, category: 'github' },
  githubSearchRepositories: { fn: githubSearchRepositories, resilience: withGitHubResilience, category: 'github' },
  githubSearchPullRequests: { fn: githubSearchPullRequests, resilience: withGitHubResilience, category: 'github' },

  // Local tools
  localSearchCode: { fn: localSearchCode, resilience: withLocalResilience, category: 'local' },
  localGetFileContent: { fn: localGetFileContent, resilience: withLocalResilience, category: 'local' },
  localFindFiles: { fn: localFindFiles, resilience: withLocalResilience, category: 'local' },
  localViewStructure: { fn: localViewStructure, resilience: withLocalResilience, category: 'local' },

  // LSP tools
  lspGotoDefinition: { fn: lspGotoDefinition, resilience: withLspResilience, category: 'lsp' },
  lspFindReferences: { fn: lspFindReferences, resilience: withLspResilience, category: 'lsp' },
  lspCallHierarchy: { fn: lspCallHierarchy, resilience: withLspResilience, category: 'lsp' },

  // Package tools
  packageSearch: { fn: packageSearch, resilience: withPackageResilience, category: 'package' },
};

/**
 * Extract repository identifiers from query parameters for session logging
 */
function extractReposFromQueries(queries: unknown[]): string[] {
  const repos: string[] = [];
  for (const query of queries) {
    const q = query as Record<string, unknown>;
    // GitHub repos: owner/repo format
    if (q.owner && q.repo) {
      repos.push(`${q.owner}/${q.repo}`);
    }
    // Local paths
    if (q.path && typeof q.path === 'string') {
      repos.push(q.path);
    }
    // LSP uris
    if (q.uri && typeof q.uri === 'string') {
      repos.push(q.uri);
    }
  }
  return [...new Set(repos)]; // Deduplicate
}

/**
 * Extract research parameters from the first query for logging
 */
function extractResearchParams(queries: unknown[]): {
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
} {
  if (queries.length === 0) return {};
  const q = queries[0] as Record<string, unknown>;
  return {
    mainResearchGoal: q.mainResearchGoal as string | undefined,
    researchGoal: q.researchGoal as string | undefined,
    reasoning: q.reasoning as string | undefined,
  };
}

/**
 * POST /tools/call/:toolName - Execute a tool directly with JSON body
 *
 * This is the SIMPLIFIED way to call tools. Instead of URL-encoded query params,
 * just POST a JSON body with your queries array.
 *
 * @example
 * POST /tools/call/localSearchCode
 * Content-Type: application/json
 *
 * {
 *   "queries": [{
 *     "mainResearchGoal": "Find authentication handlers",
 *     "researchGoal": "Locate auth middleware",
 *     "reasoning": "Understanding auth flow",
 *     "pattern": "authenticate",
 *     "path": "/Users/me/project",
 *     "type": "ts"
 *   }]
 * }
 *
 * Response:
 * {
 *   "tool": "localSearchCode",
 *   "success": true,
 *   "data": { ... parsed tool response ... },
 *   "hints": ["Use lineHint for LSP tools", ...]
 * }
 */
toolsRoutes.post('/call/:toolName', async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  // Set up AbortController for client disconnection handling
  // NOTE: req.on('close') fires when request body is consumed, not client disconnect
  // Use res.on('close') + socket.destroyed to detect actual client disconnection
  const abortController = new AbortController();
  let isAborted = false;

  res.on('close', () => {
    // Only abort if client disconnected before we finished responding
    if (!res.writableEnded && req.socket?.destroyed) {
      isAborted = true;
      abortController.abort();
    }
  });

  try {
    const { toolName } = req.params;

    // Validate tool exists
    const toolEntry = TOOL_REGISTRY[toolName];
    if (!toolEntry) {
      const availableTools = Object.keys(TOOL_REGISTRY);
      res.status(404).json({
        tool: toolName,
        success: false,
        data: null,
        hints: [
          `Tool not found: ${toolName}`,
          `Available tools: ${availableTools.join(', ')}`,
          'Check spelling or use GET /tools/list',
        ],
      });
      return;
    }

    // Validate request body using schema
    const validation = validateToolCallBody(req.body);
    if (!validation.success) {
      res.status(400).json({
        tool: toolName,
        success: false,
        data: null,
        hints: getValidationHints(toolName, validation.error!),
      });
      return;
    }

    const { queries } = validation.data!;

    // Check if request was aborted before execution
    if (isAborted) return;

    // Execute tool with resilience (includes timeout + circuit breaker + retry)
    const rawResult = await toolEntry.resilience(
      () => toolEntry.fn({ queries }),
      toolName
    );

    // Check if request was aborted after execution
    if (isAborted) return;

    // Log tool call for session telemetry
    const repos = extractReposFromQueries(queries);
    const researchParams = extractResearchParams(queries);
    fireAndForgetWithTimeout(
      () => logToolCall(
        toolName,
        repos,
        researchParams.mainResearchGoal,
        researchParams.researchGoal,
        researchParams.reasoning
      ),
      5000,
      'logToolCall'
    );

    const mcpResponse = rawResult as { content: Array<{ type: string; text: string }> };

    // For multiple queries, return bulk response format
    if (queries.length > 1) {
      const bulkParsed = parseToolResponseBulk(mcpResponse);

      res.status(bulkParsed.isError ? 500 : 200).json({
        tool: toolName,
        bulk: true,
        success: !bulkParsed.isError,
        instructions: bulkParsed.instructions,
        results: bulkParsed.results,
        hints: bulkParsed.hints,
        counts: bulkParsed.counts,
      });
      return;
    }

    // Single query - use existing response format for backward compatibility
    const parsed = parseToolResponse(mcpResponse);

    res.status(parsed.isError ? 500 : 200).json({
      tool: toolName,
      success: !parsed.isError,
      data: parsed.data,
      hints: parsed.hints,
      research: parsed.research,
    });
  } catch (error) {
    // Skip error handling if request was aborted
    if (isAborted) return;
    next(error);
  }
});
