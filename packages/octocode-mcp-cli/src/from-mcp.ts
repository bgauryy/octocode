import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport, type StdioServerParameters } from '@modelcontextprotocol/sdk/client/stdio.js';
import {
  StreamableHTTPClientTransport,
  type StreamableHTTPClientTransportOptions,
} from '@modelcontextprotocol/sdk/client/streamableHttp.js';
import { cliView, contentText, type CliView } from './run.js';
import { commandToken, defineCli, defineCommand, type CliSpec, type JsonSchema, type ToolAnnotations } from './spec.js';

const PAGE_LIMIT = 100;

export interface McpContentBlock {
  type: string;
  text?: string;
  data?: string;
  mimeType?: string;
  uri?: string;
  name?: string;
  description?: string;
  annotations?: { audience?: string[]; priority?: number; lastModified?: string };
  _meta?: Record<string, unknown>;
  resource?: { uri?: string; mimeType?: string; text?: string; blob?: string; _meta?: Record<string, unknown> };
}

export interface McpToolInfo {
  name: string;
  description?: string;
  title?: string;
  inputSchema?: JsonSchema;
  outputSchema?: JsonSchema;
  annotations?: ToolAnnotations;
}

export interface McpToolResult {
  isError?: boolean;
  structuredContent?: Record<string, unknown>;
  content?: ReadonlyArray<McpContentBlock>;
  _meta?: Record<string, unknown>;
  toolResult?: unknown;
}

export interface McpClientLike {
  getInstructions(): string | undefined;
  getServerVersion(): { name?: string; version?: string } | undefined;
  listTools(params?: { cursor?: string }): Promise<{ tools: readonly McpToolInfo[]; nextCursor?: string }>;
  callTool(params: { name: string; arguments?: Record<string, unknown> }): Promise<McpToolResult>;
}

function callView(result: McpToolResult): CliView {
  if (result.toolResult !== undefined && result.content === undefined && result.structuredContent === undefined) {
    const text = typeof result.toolResult === 'string' ? result.toolResult : JSON.stringify(result.toolResult, null, 2);
    return cliView(text, { toolResult: result.toolResult });
  }
  if (result.isError) throw new Error(contentText(result.content ?? []) || 'tool error');
  const json: Record<string, unknown> = {};
  if (result._meta !== undefined) json._meta = result._meta;
  if (result.content !== undefined) json.content = result.content;
  if (result.structuredContent !== undefined) json.structuredContent = result.structuredContent;
  json.isError = false;
  if (result.structuredContent) {
    return cliView(JSON.stringify(result.structuredContent, null, 2), json);
  }
  return cliView(contentText(result.content ?? []), json);
}

async function callNamedTool(
  client: McpClientLike,
  name: string,
  input: Record<string, unknown>,
): Promise<CliView> {
  return callView(await client.callTool({ name, arguments: input }));
}

function stringField(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0 ? value : undefined;
}

function annotationsOf(value: ToolAnnotations | undefined): ToolAnnotations | undefined {
  if (!value) return undefined;
  const annotations: ToolAnnotations = {};
  if (typeof value.title === 'string') annotations.title = value.title;
  if (typeof value.readOnlyHint === 'boolean') annotations.readOnlyHint = value.readOnlyHint;
  if (typeof value.destructiveHint === 'boolean') annotations.destructiveHint = value.destructiveHint;
  if (typeof value.idempotentHint === 'boolean') annotations.idempotentHint = value.idempotentHint;
  if (typeof value.openWorldHint === 'boolean') annotations.openWorldHint = value.openWorldHint;
  return Object.keys(annotations).length > 0 ? annotations : undefined;
}

function copyValue(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(copyValue);
  if (!value || typeof value !== 'object') return value;
  const copy: Record<string, unknown> = {};
  for (const [key, child] of Object.entries(value)) {
    if (child !== undefined) copy[key] = copyValue(child);
  }
  return copy;
}

function contentBlock(block: { type: string }): McpContentBlock {
  return copyValue(block) as McpContentBlock;
}

function asProtocol(client: Client): McpClientLike {
  return {
    getInstructions: () => client.getInstructions(),
    getServerVersion: () => client.getServerVersion(),
    async listTools(params) {
      const listed = await client.listTools(params);
      return {
        nextCursor: listed.nextCursor,
        tools: listed.tools.map(tool => ({
          name: tool.name,
          description: tool.description,
          title: tool.title,
          inputSchema: tool.inputSchema as JsonSchema | undefined,
          outputSchema: tool.outputSchema as JsonSchema | undefined,
          annotations: tool.annotations,
        })),
      };
    },
    async callTool(params) {
      const result = await client.callTool({ name: params.name, arguments: params.arguments });
      if ('toolResult' in result && !('content' in result)) return { toolResult: result.toolResult };
      const call = result as {
        isError?: boolean;
        structuredContent?: Record<string, unknown>;
        content?: ReadonlyArray<{ type: string }>;
        _meta?: Record<string, unknown>;
      };
      return {
        ...(call._meta !== undefined ? { _meta: copyValue(call._meta) as Record<string, unknown> } : {}),
        isError: call.isError,
        ...(call.structuredContent !== undefined
          ? { structuredContent: copyValue(call.structuredContent) as Record<string, unknown> }
          : {}),
        content: call.content?.map(contentBlock),
      };
    },
  };
}

function uniqueToken(name: string, used: Set<string>): { token: string; aliased: boolean } {
  const first = commandToken(name);
  if (!used.has(first.token)) {
    used.add(first.token);
    return first;
  }
  let index = 2;
  let token = `${first.token}-${index}`;
  while (used.has(token)) {
    index += 1;
    token = `${first.token}-${index}`;
  }
  used.add(token);
  return { token, aliased: true };
}

export async function cliFromMcp(client: Client | McpClientLike): Promise<CliSpec> {
  return cliFromProtocol(client instanceof Client ? asProtocol(client) : client);
}

async function cliFromProtocol(client: McpClientLike): Promise<CliSpec> {
  const tools: McpToolInfo[] = [];
  const seen = new Set<string>();
  let cursor: string | undefined;
  for (let page = 0; page < PAGE_LIMIT; page += 1) {
    if (cursor !== undefined) {
      if (seen.has(cursor)) throw new Error('Repeated tools/list cursor');
      seen.add(cursor);
    }
    const listed = await client.listTools(cursor ? { cursor } : undefined);
    tools.push(...listed.tools);
    const next = listed.nextCursor;
    if (typeof next !== 'string' || next.length === 0) {
      cursor = undefined;
      break;
    }
    cursor = next;
  }
  if (cursor !== undefined) throw new Error('tools/list page limit');

  const used = new Set<string>();
  const commands = tools.map(tool => {
    const named = uniqueToken(tool.name, used);
    const inputSchema = tool.inputSchema ?? { type: 'object', properties: {} };
    const annotations = annotationsOf(tool.annotations);
    const title = stringField(tool.title) ?? stringField(annotations?.title);
    return defineCommand({
      name: named.token,
      description: tool.description ?? '',
      ...(title ? { title } : {}),
      source: 'mcp',
      inputSchema,
      ...(tool.outputSchema ? { outputSchema: tool.outputSchema } : {}),
      ...(annotations ? { annotations } : {}),
      ...(named.token !== tool.name ? { mcpName: tool.name } : {}),
      run: input => callNamedTool(client, tool.name, input),
    });
  });
  const info = client.getServerVersion();
  return defineCli({
    name: info?.name && info.name.length > 0 ? info.name : 'mcp',
    instructions: client.getInstructions() ?? '',
    version: info?.version && info.version.length > 0 ? info.version : '0.0.0',
    commands,
  });
}

export function createStdioTransport(parameters: StdioServerParameters): StdioClientTransport {
  return new StdioClientTransport(parameters);
}

export function createStreamableHttpTransport(
  url: string | URL,
  options?: StreamableHTTPClientTransportOptions,
): StreamableHTTPClientTransport {
  const target = typeof url === 'string' ? new URL(url) : url;
  return new StreamableHTTPClientTransport(target, options);
}

export async function connectClient(transport: Parameters<Client['connect']>[0]): Promise<Client> {
  const client = new Client({ name: 'octocode-mcp-cli', version: '0.1.0' });
  await client.connect(transport);
  return client;
}

export async function connectStdio(
  parameters: StdioServerParameters,
  connect: (transport: StdioClientTransport) => Promise<Client> = connectClient,
): Promise<Client> {
  return connect(createStdioTransport(parameters));
}

export async function connectStreamableHttp(
  url: string | URL,
  options?: StreamableHTTPClientTransportOptions,
  connect: (transport: StreamableHTTPClientTransport) => Promise<Client> = connectClient,
): Promise<Client> {
  return connect(createStreamableHttpTransport(url, options));
}
