import type { PiContext, ToolCallResult } from '../../types.js';
import { recordFileReadState } from '../file-state.js';
import { formatMcpSchemaValidationErrors } from './presentation.js';
import { McpSchemaUnsupportedError } from './schema-validator.js';
import { collectMcpPages, type McpCursorPage } from './pagination.js';
import { resolveMcpCallContent, resolveMcpCallTable, summarizeMcpCallDetails, stringify } from './sanitize.js';
import {
  DEFAULT_OCTOCODE_MCP_SERVER_NAME,
  isPlainRecord,
  requestOptions,
  type McpLoadedConfig,
  type McpServerConfig,
} from './config.js';
import type { McpAction, McpConnection, ValidatedMcpTool } from './types.js';

export type EnsureMcpConnection = (
  name: string,
  config: McpServerConfig,
  ctx?: PiContext,
  signal?: AbortSignal,
) => Promise<McpConnection>;

type ValidateMcpTool = (
  target: { server: string; tool: string },
  loaded: McpLoadedConfig,
  ctx?: PiContext,
  signal?: AbortSignal,
) => Promise<ValidatedMcpTool>;

function result(text: string, details?: unknown, isError = false): ToolCallResult {
  return { content: [{ type: 'text', text }], details, isError };
}

export async function executeMcpResourceAction(options: {
  action: Extract<McpAction, 'resources' | 'read-resource' | 'prompts' | 'get-prompt' | 'complete'>;
  params: Record<string, unknown>;
  serverName: string;
  loaded: McpLoadedConfig;
  signal?: AbortSignal;
  ctx?: PiContext;
  ensureConnection: EnsureMcpConnection;
}): Promise<ToolCallResult> {
  const { action, params, serverName, loaded, signal, ctx, ensureConnection } = options;
  const config = loaded.servers.get(serverName)!;
  const connection = await ensureConnection(serverName, config, ctx, signal);
  let payload: unknown;

  if (action === 'resources') {
    const [resources, resourceTemplates] = await Promise.all([
      collectMcpPages<unknown>(
        `${serverName} resources/list`,
        async cursor => connection.client.listResources(
          cursor ? { cursor } : undefined,
          requestOptions(config, signal),
        ) as Promise<McpCursorPage>,
        page => Array.isArray((page as Record<string, unknown>)['resources'])
          ? (page as Record<string, unknown>)['resources'] as unknown[]
          : [],
      ),
      collectMcpPages<unknown>(
        `${serverName} resources/templates/list`,
        async cursor => connection.client.listResourceTemplates(
          cursor ? { cursor } : undefined,
          requestOptions(config, signal),
        ) as Promise<McpCursorPage>,
        page => Array.isArray((page as Record<string, unknown>)['resourceTemplates'])
          ? (page as Record<string, unknown>)['resourceTemplates'] as unknown[]
          : [],
      ),
    ]);
    payload = { resources, resourceTemplates };
  } else if (action === 'read-resource') {
    const uri = typeof params['uri'] === 'string' ? params['uri'] : '';
    if (!uri) return result('MCPTool read-resource requires uri', undefined, true);
    payload = await connection.client.readResource({ uri }, requestOptions(config, signal));
  } else if (action === 'prompts') {
    const prompts = await collectMcpPages<unknown>(
      `${serverName} prompts/list`,
      async cursor => connection.client.listPrompts(
        cursor ? { cursor } : undefined,
        requestOptions(config, signal),
      ) as Promise<McpCursorPage>,
      page => Array.isArray((page as Record<string, unknown>)['prompts'])
        ? (page as Record<string, unknown>)['prompts'] as unknown[]
        : [],
    );
    payload = { prompts };
  } else if (action === 'get-prompt') {
    const name = typeof params['name'] === 'string' ? params['name'] : '';
    if (!name) return result('MCPTool get-prompt requires name', undefined, true);
    payload = await connection.client.getPrompt(
      {
        name,
        arguments: isPlainRecord(params['arguments'])
          ? params['arguments'] as Record<string, string>
          : undefined,
      },
      requestOptions(config, signal),
    );
  } else {
    if (!isPlainRecord(params['ref']) || !isPlainRecord(params['argument'])) {
      return result('MCPTool complete requires ref and argument objects', undefined, true);
    }
    payload = await connection.client.complete(
      { ref: params['ref'] as never, argument: params['argument'] as never },
      requestOptions(config, signal),
    );
  }

  return result(stringify(payload), payload);
}

export async function executeMcpToolCall(options: {
  params: Record<string, unknown>;
  serverName: string;
  loaded: McpLoadedConfig;
  signal?: AbortSignal;
  ctx?: PiContext;
  ensureConnection: EnsureMcpConnection;
  validateTool: ValidateMcpTool;
  schemaMetrics: { blockedCalls: number };
}): Promise<ToolCallResult> {
  const {
    params,
    serverName,
    loaded,
    signal,
    ctx,
    ensureConnection,
    validateTool,
    schemaMetrics,
  } = options;
  const tool = params['tool'];
  if (typeof tool !== 'string' || tool.trim().length === 0) {
    return result('MCPTool call requires tool', undefined, true);
  }
  const config = loaded.servers.get(serverName)!;
  const argumentsPayload = isPlainRecord(params['arguments']) ? params['arguments'] : {};
  let validated: ValidatedMcpTool;
  try {
    validated = await validateTool({ server: serverName, tool }, loaded, ctx, signal);
  } catch (error) {
    const code = error instanceof McpSchemaUnsupportedError
      ? error.code
      : 'SCHEMA_UNAVAILABLE';
    return result(
      `${code} ${serverName}/${tool}\n${(error as Error).message}`,
      { server: serverName, tool },
      true,
    );
  }

  const expectedSchemaDigest = params['__expectedSchemaDigest'];
  if (typeof expectedSchemaDigest === 'string' && expectedSchemaDigest !== validated.schemaDigest) {
    schemaMetrics.blockedCalls += 1;
    return result(
      `MCP_SCHEMA_STALE ${serverName}/${tool}\nThe active Pi proxy was compiled for a previous schema revision. Run MCPTool action:"describe" again.`,
      {
        server: serverName,
        tool,
        expectedSchemaDigest,
        currentSchemaDigest: validated.schemaDigest,
      },
      true,
    );
  }

  const validation = validated.validator.validate(argumentsPayload);
  if (!validation.valid) {
    schemaMetrics.blockedCalls += 1;
    return result(
      `MCP_SCHEMA_INVALID ${serverName}/${tool}\n${formatMcpSchemaValidationErrors(validation.errors, { server: serverName, tool })}`,
      {
        server: serverName,
        tool,
        inputSchema: validated.inputSchema,
        errors: validation.errors,
      },
      true,
    );
  }

  const connection = await ensureConnection(serverName, config, ctx, signal);
  const payload = await connection.client.callTool(
    { name: tool, arguments: argumentsPayload },
    requestOptions(config, signal),
  );
  if (
    serverName === DEFAULT_OCTOCODE_MCP_SERVER_NAME &&
    tool === 'localFetch' &&
    payload?.isError !== true
  ) {
    const cwd = ctx?.cwd ?? process.cwd();
    const queries = Array.isArray(argumentsPayload['queries']) ? argumentsPayload['queries'] : [];
    const structured = payload?.structuredContent;
    const readResults = isPlainRecord(structured) && Array.isArray(structured['results'])
      ? structured['results']
      : [];
    await Promise.all(readResults.map(async (entry: unknown) => {
      if (!isPlainRecord(entry) || entry['status'] !== undefined) return;
      const index = entry['index'];
      const data = entry['data'];
      if (
        typeof index !== 'number' || !Number.isInteger(index) || index < 0 ||
        !isPlainRecord(data) || typeof data['content'] !== 'string'
      ) return;
      const query: unknown = queries[index];
      const filePath = isPlainRecord(query) ? query['path'] : undefined;
      if (typeof filePath === 'string' && filePath.trim().length > 0) {
        await recordFileReadState(filePath, cwd).catch(() => undefined);
      }
    }));
  }

  const tableContent = params['responseView'] === 'table'
    ? resolveMcpCallTable(payload)
    : undefined;
  return {
    content: tableContent ?? resolveMcpCallContent(payload),
    details: summarizeMcpCallDetails(payload),
    isError: payload?.isError === true,
  };
}
