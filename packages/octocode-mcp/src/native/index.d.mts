// Type surface for the native MCP bridge (implemented in index.mjs). Only the
// members consumed by TypeScript callers are declared here.
export interface NativeMcpInstance {
  server: {
    connect(transport: unknown): Promise<void>;
    close(): Promise<void>;
  };
  runtime: unknown;
  catalog: unknown;
  close(): Promise<void>;
}

export function loadNativeBinding(env?: NodeJS.ProcessEnv): {
  NativeRuntime: new (options?: unknown) => unknown;
};

export function createNativeMcp(options?: {
  env?: NodeJS.ProcessEnv;
  binding?: { NativeRuntime: new (options?: unknown) => unknown };
}): NativeMcpInstance;

export function startNativeMcp(options?: {
  env?: NodeJS.ProcessEnv;
  binding?: { NativeRuntime: new (options?: unknown) => unknown };
}): Promise<NativeMcpInstance>;
