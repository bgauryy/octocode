/** Mirrors NATIVE_ABI_VERSION in crates/runtime/src/lib.rs. */
export declare const NATIVE_ABI_VERSION: number;

export interface NativeRuntimeOptions {
  surface?: string;
  regexWorkerPath?: string;
  [key: string]: unknown;
}

export declare class NativeRuntime {
  constructor(options?: NativeRuntimeOptions | null);
  readonly abiVersion: number;
  readonly closed: boolean;
  catalog(): unknown;
  execute(requestId: string, tool: string, input: unknown): Promise<unknown>;
  executeMcp(requestId: string, tool: string, input: unknown): Promise<unknown>;
  cancel(requestId: string): boolean;
  close(): Promise<void>;
  storeCredentials(value: unknown): unknown;
  getCredentials(hostname?: string | null): unknown;
  deleteCredentials(hostname?: string | null): unknown;
  refreshAuthToken(hostname?: string | null): Promise<unknown>;
  getTokenWithRefresh(hostname?: string | null): Promise<unknown>;
}

declare const runtimeBinding: {
  NativeRuntime: typeof NativeRuntime;
};

export default runtimeBinding;
