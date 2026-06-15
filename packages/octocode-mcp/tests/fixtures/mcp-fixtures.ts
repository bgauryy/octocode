import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import {
  CallToolRequest,
  CallToolResult,
} from '@modelcontextprotocol/sdk/types.js';
import { vi } from 'vitest';

export interface MockMcpServer {
  server: McpServer;
  registrations: MockToolRegistration[];
  callTool: (
    name: string,
    args?: Record<string, unknown>,
    options?: {
      authInfo?: { token?: string };
      sessionId?: string;
      signal?: AbortSignal;
    }
  ) => Promise<CallToolResult>;
  cleanup: () => void;
}

export interface MockToolRegistration {
  name: string;
  options: unknown;
  handler: Function;
}

export function createMockMcpServer(): MockMcpServer {
  const toolHandlers = new Map<string, Function>();
  const registrations: MockToolRegistration[] = [];

  const mockServer = {
    tool: vi.fn((name: string, handler: Function) => {
      toolHandlers.set(name, handler);
    }),

    registerTool: vi.fn((name: string, options: unknown, handler: Function) => {
      registrations.push({ name, options, handler });
      toolHandlers.set(name, handler);
    }),
    addTool: vi.fn(),
    listTools: vi.fn(),
  } as unknown as McpServer;

  const callTool = async (
    name: string,
    args?: Record<string, unknown>,
    options?: {
      authInfo?: { token?: string };
      sessionId?: string;
      signal?: AbortSignal;
    }
  ): Promise<CallToolResult> => {
    const handler = toolHandlers.get(name);
    if (!handler) {
      throw new Error(`Tool '${name}' not found`);
    }

    const request: CallToolRequest = {
      method: 'tools/call',
      params: {
        name,
        arguments: args || {},
      },
    };

    return await handler(request.params.arguments, {
      authInfo: options?.authInfo,
      sessionId: options?.sessionId,
      signal: options?.signal,
    });
  };

  const cleanup = () => {
    toolHandlers.clear();
    registrations.length = 0;
    vi.clearAllMocks();
  };

  return {
    server: mockServer,
    registrations,
    callTool,
    cleanup,
  };
}

export function createMockResult(
  data: unknown,
  isError = false
): CallToolResult {
  return {
    content: [
      {
        type: 'text',
        text: isError ? String(data) : JSON.stringify(data, null, 2),
      },
    ],
    isError,
  };
}

export function parseResultJson<T = unknown>(result: CallToolResult): T {
  if (result.isError || !result.content?.[0]) {
    throw new Error('Cannot parse error result');
  }

  const firstContent = result.content[0];
  if (firstContent.type !== 'text') {
    throw new Error('Content is not text type');
  }

  const text = firstContent.text;
  if (typeof text !== 'string') {
    throw new Error('Result content is not a string');
  }

  return JSON.parse(text) as T;
}
