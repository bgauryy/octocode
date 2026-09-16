import type { Client, Transport } from '@modelcontextprotocol/client';
import type { McpServerConfig, McpConfigSource } from './config.js';
import type { McpOAuthFlow } from './oauth.js';
import type { McpCompiledSchemaValidator } from './schema-validator.js';


export type McpAction =
  | "list"
  | "describe"
  | "call"
  | "resources"
  | "read-resource"
  | "prompts"
  | "get-prompt"
  | "complete"
  | "enable"
  | "disable"
  | "status"
  | "restart"
  | "stop"
  | "config"
  | "add"
  | "remove";

export interface McpConnection {
  name: string;
  config: McpServerConfig;
  /** Stable signature of the normalized config; used to auto-reconnect on config drift. */
  configSig: string;
  client: Client;
  transport: Transport;
  stderr: string[];
  startedAt: number;
  oauth?: McpOAuthFlow;
}

export interface ListedMcpServer {
  name: string;
  instructions?: string;
  tools: unknown[];
  text: string;
  configSignature?: string;
  /** Fetch time for diagnostics only; never render it in eager catalog or lazy index bytes. */
  cachedAt?: number;
}

export interface ValidatedMcpTool {
  server: string;
  tool: string;
  instructions?: string;
  inputSchema: unknown;
  schemaDigest: string;
  validator: McpCompiledSchemaValidator;
}

export interface McpDiscoveryServer {
  name: string;
  command: string;
  args: string[];
  description?: string;
  /** Present only for servers whose catalog was discovered (warmed/listed). */
  toolCount?: number;
  tools?: Array<{ name: string; description: string }>;
}

export interface McpDiscoverySnapshot {
  sources: McpConfigSource[];
  servers: McpDiscoveryServer[];
  warnings: string[];
}

export interface McpPromptArtifactStatus {
  mode: "routing";
  status: "pending" | "ready";
  promptChars: number;
  workspaceKey?: string;
  configDigest?: string;
  capturedAt?: string;
  catalogPath?: string;
}
