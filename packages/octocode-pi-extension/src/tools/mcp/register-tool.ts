import type { PiContext, PiInstance, ToolCallResult, ToolDefinition } from '../../types.js';
import { DIRECT_TOOL_DESCRIPTIONS } from '../octocode-tools.js';
import { isWorkerCapabilityClient, getCurrentWorkerCapabilities } from '../worker-capabilities.js';
import { buildQueryEnvelopeSchema, executeQueryBatch, type QueryRecord } from '../query-envelope.js';
import { QueryBatchError } from '../query-batch-error.js';
import { setManagedStatus } from '../runtime-renderer.js';
import { mcpGatewayItemSchema } from './gateway-contract.js';
import { renderMcpCall, renderMcpResult, summarizeMcpBatchResult } from './presentation.js';
import {
  activateDescribedMcpProxy,
  createDynamicMcpProxyState,
  mcpToolIdentity,
} from './dynamic-proxy.js';
import type { McpAction } from './types.js';

const MCP_STATUS_NAME = 'octocode-mcp';

type HandleMcpAction = (
  params: Record<string, unknown>,
  signal?: AbortSignal,
  ctx?: PiContext,
) => Promise<ToolCallResult>;

function textResult(text: string, isError = false): ToolCallResult {
  return { content: [{ type: 'text', text }], isError };
}

function schemaRequiredMessage(server: string, tool: string): string {
  return [
    `MCP_SCHEMA_REQUIRED ${server}/${tool}`,
    'Load the exact schema before calling through MCPTool:',
    JSON.stringify({
      tool: 'MCPTool',
      params: { queries: [{ action: 'describe', server, tool }] },
    }),
  ].join('\n');
}

export function registerMcpGatewayTool(
  pi: PiInstance,
  registeredToolNames: Set<string>,
  registerFn: (
    pi: PiInstance,
    registeredToolNames: Set<string>,
    toolDefinition: ToolDefinition,
  ) => void,
  handleMcpAction: HandleMcpAction,
  preflightMcpQuery: (query: QueryRecord) => void,
): void {
  const proxyState = createDynamicMcpProxyState(pi);
  const itemSchema = mcpGatewayItemSchema();
  const parameters = buildQueryEnvelopeSchema(itemSchema, { allowParallel: true });

  const execute = async (
    toolCallId: string,
    params: Record<string, unknown>,
    signal?: AbortSignal,
    onUpdate?: unknown,
    ctx?: PiContext,
  ): Promise<ToolCallResult> => {
    setManagedStatus(ctx, MCP_STATUS_NAME, 'mcp · running');
    const parallelServers = new Set<string>();
    try {
      const output = await executeQueryBatch({
        toolCallId,
        raw: params,
        signal,
        onUpdate:
          typeof onUpdate === 'function'
            ? (onUpdate as (update: ToolCallResult) => void)
            : undefined,
        ctx,
        passthroughSingle: true,
        allowParallel: true,
        async preflight(query) {
          preflightMcpQuery(query);
          if (proxyState.supported && query['action'] === 'call') {
            const server = String(query['server'] ?? '');
            const tool = String(query['tool'] ?? '');
            const workerView = isWorkerCapabilityClient()
              ? getCurrentWorkerCapabilities()
              : undefined;
            const granted = !workerView || workerView.snapshot.mcpTools.some(
              candidate => candidate.server === server && candidate.tool === tool,
            );
            const describedDigest = proxyState.describedSchemas.get(mcpToolIdentity(server, tool));
            if (granted && !describedDigest) {
              const describeResult = await handleMcpAction(
                { action: 'describe', server, tool },
                signal,
                ctx,
              );
              if (describeResult.isError) {
                const errorText = describeResult.content
                  .filter((part): part is { type: 'text'; text: string } => part.type === 'text')
                  .map(part => part.text)
                  .join('\n');
                throw new Error(errorText || schemaRequiredMessage(server, tool));
              }
              activateDescribedMcpProxy(
                pi,
                proxyState,
                describeResult,
                handleMcpAction,
                ctx,
              );
            }
          }
          if (params['queryRunType'] !== 'parallel') return;
          const action = query['action'] as McpAction;
          if (![
            'status',
            'describe',
            'call',
            'resources',
            'read-resource',
            'prompts',
            'get-prompt',
            'complete',
          ].includes(action)) {
            throw new Error(`parallel MCP batches do not support the mutating ${action} action`);
          }
          const server = typeof query['server'] === 'string' ? query['server'] : undefined;
          if (server && parallelServers.has(server)) {
            throw new Error(
              `parallel MCP batches require distinct servers; batch same-server ${server} queries inside the target tool arguments`,
            );
          }
          if (server) parallelServers.add(server);
        },
        async execute(query, _index, _itemId, batchSignal, _onItemUpdate, itemCtx) {
          const action = query['action'] as McpAction;
          const identity = action === 'call'
            ? mcpToolIdentity(String(query['server'] ?? ''), String(query['tool'] ?? ''))
            : undefined;
          const expectedSchemaDigest = identity
            ? proxyState.describedSchemas.get(identity)
            : undefined;
          const actionResult = await handleMcpAction(
            expectedSchemaDigest
              ? { ...query, __expectedSchemaDigest: expectedSchemaDigest }
              : query,
            batchSignal,
            itemCtx,
          );
          return action === 'describe'
            ? activateDescribedMcpProxy(
              pi,
              proxyState,
              actionResult,
              handleMcpAction,
              itemCtx,
            )
            : actionResult;
        },
        summarize: summarizeMcpBatchResult,
      });
      if (isWorkerCapabilityClient() && output.isError) {
        throw new Error(
          output.content
            .filter(part => part.type === 'text')
            .map(part => part.text)
            .join('\n'),
        );
      }
      return output;
    } catch (error) {
      if (isWorkerCapabilityClient()) throw error;
      if (error instanceof QueryBatchError && error.completedCount > 0) throw error;
      return textResult(`[MCP_ERROR] ${(error as Error).message}`, true);
    } finally {
      setManagedStatus(ctx, MCP_STATUS_NAME, undefined);
    }
  };

  const common = {
    label: 'MCPTool',
    description: DIRECT_TOOL_DESCRIPTIONS.MCPTool!,
    promptSnippet: 'Gateway to MCP servers and exact-schema loader. Built-in octocode catalog in <mcp_catalog_index>.',
    promptGuidelines: [
      'Select from <mcp_catalog_index>; omit server for Octocode. Describe only when the target schema is not active, then call the activated Pi tool. Generic action:call requires that same described schema.',
      'Batch independent target queries inside arguments.queries[]. Use outer parallel execution only across different servers.',
      'add/remove writes mcp.json; restart/stop manages connections. Do not add untrusted MCP config without user approval.',
    ],
    parameters,
    execute,
    renderCall: renderMcpCall,
    renderResult: renderMcpResult,
  } satisfies Omit<ToolDefinition, 'name'>;

  registerFn(pi, registeredToolNames, { name: 'MCPTool', ...common });
}
