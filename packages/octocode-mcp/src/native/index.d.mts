import type { McpServer } from '@modelcontextprotocol/server';

export interface NativeRuntimeBinding {
  NativeRuntime: new (options?: Record<string, unknown>) => {
    readonly abiVersion: number;
    catalog(): NativeCatalog;
    executeMcp(
      requestId: string,
      tool: string,
      input: unknown
    ): Promise<unknown>;
    cancel(requestId: string): boolean;
    close(): Promise<void>;
  };
}

export interface NativeCatalogTool {
  name: string;
  title?: string;
  description?: string;
  available: boolean;
  inputSchema: unknown;
  outputSchema?: unknown;
  annotations?: unknown;
}

export interface NativeCatalog {
  fingerprint: string;
  server?: { name: string; title?: string; version: string };
  mcpInstructions?: string;
  tools: NativeCatalogTool[];
}

export function loadNativeBinding(
  env?: NodeJS.ProcessEnv
): NativeRuntimeBinding;

export function createNativeMcp(options?: {
  env?: NodeJS.ProcessEnv;
  binding?: NativeRuntimeBinding;
}): {
  server: McpServer;
  runtime: InstanceType<NativeRuntimeBinding['NativeRuntime']>;
  catalog: NativeCatalog;
  close(): Promise<void>;
};

export function startNativeMcp(options?: {
  env?: NodeJS.ProcessEnv;
  binding?: NativeRuntimeBinding;
}): Promise<ReturnType<typeof createNativeMcp>>;
