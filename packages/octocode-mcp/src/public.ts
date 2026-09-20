import type { McpServer } from '@modelcontextprotocol/server';
import {
  createNativeMcp as createNativeMcpImpl,
  loadNativeBinding as loadNativeBindingImpl,
  startNativeMcp as startNativeMcpImpl,
} from './native/index.mjs';

export interface NativeRuntime {
  readonly abiVersion: number;
  catalog(): NativeCatalog;
  executeMcp(requestId: string, tool: string, input: unknown): Promise<unknown>;
  cancel(requestId: string): boolean;
  close(): Promise<void>;
}

export interface NativeRuntimeBinding {
  NativeRuntime: new (options?: Record<string, unknown>) => NativeRuntime;
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

export interface NativeMcp {
  server: McpServer;
  runtime: NativeRuntime;
  catalog: NativeCatalog;
  close(): Promise<void>;
}

export interface NativeMcpOptions {
  env?: NodeJS.ProcessEnv;
  binding?: NativeRuntimeBinding;
}

export function loadNativeBinding(
  env?: NodeJS.ProcessEnv
): NativeRuntimeBinding {
  return loadNativeBindingImpl(env) as NativeRuntimeBinding;
}

export function createNativeMcp(options?: NativeMcpOptions): NativeMcp {
  return createNativeMcpImpl(options) as NativeMcp;
}

export async function startNativeMcp(
  options?: NativeMcpOptions
): Promise<NativeMcp> {
  return (await startNativeMcpImpl(options)) as NativeMcp;
}
