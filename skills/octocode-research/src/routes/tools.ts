import { Router, type Request, type Response, type NextFunction } from 'express';
import { getMcpContent } from '../mcpCache.js';
import { transformToJsonSchema } from '../types/mcp.js';
import { z } from 'zod';

import {
  GitHubCodeSearchQuerySchema,
  GitHubViewRepoStructureQuerySchema,
  GitHubReposSearchSingleQuerySchema,
  GitHubPullRequestSearchQuerySchema,
  CloneRepoQuerySchema,
  FileContentQuerySchema,
  RipgrepQuerySchema,
  FetchContentQuerySchema,
  FindFilesQuerySchema,
  ViewStructureQuerySchema,
  LspGetSemanticsQuerySchema,
  NpmSearchQuerySchema,
} from 'octocode-mcp/public';
import {
  ghSearchCode,
  ghGetFileContent,
  ghViewRepoStructure,
  ghSearchRepos,
  ghSearchPRs,
  ghCloneRepo,
  npmSearch,
  localSearchCode,
  localGetFileContent,
  localFindFiles,
  localViewStructure,
  lspGetSemantics,
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

toolsRoutes.use(checkReadiness);

declare const __PACKAGE_VERSION__: string;
const PACKAGE_VERSION = __PACKAGE_VERSION__;

interface ToolsInfoQuery {
  schema?: string;
  hints?: string;
}


const TOOL_ZOD_SCHEMAS: Record<string, z.ZodType> = {
  ghSearchCode: GitHubCodeSearchQuerySchema,
  ghGetFileContent: FileContentQuerySchema,
  ghViewRepoStructure: GitHubViewRepoStructureQuerySchema,
  ghSearchRepos: GitHubReposSearchSingleQuerySchema,
  ghSearchPRs: GitHubPullRequestSearchQuerySchema,
  ghCloneRepo: CloneRepoQuerySchema,
  localSearchCode: RipgrepQuerySchema,
  localGetFileContent: FetchContentQuerySchema,
  localFindFiles: FindFilesQuerySchema,
  localViewStructure: ViewStructureQuerySchema,
  lspGetSemantics: LspGetSemanticsQuerySchema,
  npmSearch: NpmSearchQuerySchema,
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


toolsRoutes.get('/list', (_req: Request, res: Response) => {
  res.json({
    success: true,
    data: {
      tools: [
        { name: 'ghSearchCode', description: 'Search code in GitHub repos' },
        { name: 'ghGetFileContent', description: 'Read file from GitHub repo' },
        { name: 'ghViewRepoStructure', description: 'View GitHub repo tree' },
        { name: 'ghSearchRepos', description: 'Search GitHub repositories' },
        { name: 'ghSearchPRs', description: 'Search pull requests' },
        { name: 'ghCloneRepo', description: 'Clone GitHub repos or subtrees for local analysis' },
        { name: 'npmSearch', description: 'Search npm packages' },
        { name: 'localSearchCode', description: 'Search local code with ripgrep' },
        { name: 'localGetFileContent', description: 'Read local file content' },
        { name: 'localFindFiles', description: 'Find files by pattern/metadata' },
        { name: 'localViewStructure', description: 'View local directory tree' },
        { name: 'lspGetSemantics', description: 'Go to definition, find references, call hierarchy, hover, document symbols' },
      ],
    },
    hints: ['GET /tools/info/{name} for full schema before calling'],
  });
});


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

toolsRoutes.get('/info/:toolName', async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const content = getMcpContent();

    const { toolName } = req.params;
    const query = req.query as ToolsInfoQuery;
    const includeSchema = query.schema !== 'false';
    const includeHints = query.hints !== 'false';

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
      const zodJsonSchema = getToolJsonSchema(toolName);
      if (zodJsonSchema) {
        result.inputSchema = zodJsonSchema;
        result._schemaSource = 'zod';
      } else {
        result.inputSchema = transformToJsonSchema(tool.schema, tool.name);
        result._schemaSource = 'metadata';
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

toolsRoutes.get('/initContext', async (
  _req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const content = getMcpContent();

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


type ResilienceFn = <T>(fn: () => Promise<T>, toolName: string) => Promise<T>;


interface ToolEntry {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  fn: (params: any) => Promise<any>;
  resilience: ResilienceFn;
  category: 'github' | 'local' | 'lsp' | 'package';
}

const TOOL_REGISTRY: Record<string, ToolEntry> = {
  ghSearchCode: { fn: ghSearchCode, resilience: withGitHubResilience, category: 'github' },
  ghGetFileContent: { fn: ghGetFileContent, resilience: withGitHubResilience, category: 'github' },
  ghViewRepoStructure: { fn: ghViewRepoStructure, resilience: withGitHubResilience, category: 'github' },
  ghSearchRepos: { fn: ghSearchRepos, resilience: withGitHubResilience, category: 'github' },
  ghSearchPRs: { fn: ghSearchPRs, resilience: withGitHubResilience, category: 'github' },
  ghCloneRepo: { fn: ghCloneRepo, resilience: withGitHubResilience, category: 'github' },

  localSearchCode: { fn: localSearchCode, resilience: withLocalResilience, category: 'local' },
  localGetFileContent: { fn: localGetFileContent, resilience: withLocalResilience, category: 'local' },
  localFindFiles: { fn: localFindFiles, resilience: withLocalResilience, category: 'local' },
  localViewStructure: { fn: localViewStructure, resilience: withLocalResilience, category: 'local' },

  lspGetSemantics: { fn: lspGetSemantics, resilience: withLspResilience, category: 'lsp' },

  npmSearch: { fn: npmSearch, resilience: withPackageResilience, category: 'package' },
};


function extractReposFromQueries(queries: unknown[]): string[] {
  const repos: string[] = [];
  for (const query of queries) {
    const q = query as Record<string, unknown>;
    if (q.owner && q.repo) {
      repos.push(`${q.owner}/${q.repo}`);
    }
    if (q.path && typeof q.path === 'string') {
      repos.push(q.path);
    }
    if (q.uri && typeof q.uri === 'string') {
      repos.push(q.uri);
    }
  }
  return [...new Set(repos)];
}


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

toolsRoutes.post('/call/:toolName', async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  const abortController = new AbortController();
  let isAborted = false;

  res.on('close', () => {
    if (!res.writableEnded && req.socket?.destroyed) {
      isAborted = true;
      abortController.abort();
    }
  });

  try {
    const { toolName } = req.params;

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

    if (isAborted) return;

    const rawResult = await toolEntry.resilience(
      () => toolEntry.fn({ queries }),
      toolName
    );

    if (isAborted) return;

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

    const parsed = parseToolResponse(mcpResponse);

    res.status(parsed.isError ? 500 : 200).json({
      tool: toolName,
      success: !parsed.isError,
      data: parsed.data,
      hints: parsed.hints,
      research: parsed.research,
    });
  } catch (error) {
    if (isAborted) return;
    next(error);
  }
});
