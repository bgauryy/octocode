/**
 * The addon ABI this package's JS expects (`NativeRuntime.abiVersion` must
 * match). Defined once in `runtime.cjs`; mirrors NATIVE_ABI_VERSION in
 * crates/runtime/src/lib.rs.
 */
export declare const NATIVE_ABI_VERSION: number;

/** Host options; the runtime rejects unknown keys. */
export interface NativeRuntimeOptions {
  /** Runtime surface, e.g. `mcp` (config `RuntimeSurface`). */
  surface?: string;
  cwd?: string;
  regexWorkerPath?: string;
  /** Explicit host environment; standalone surfaces fall back to process env. */
  env?: Record<string, string>;
  trustedProject?: boolean;
  /** Per-request execution timeout in seconds. */
  timeoutSecs?: number;
}

/** A tool in the runtime catalog: name and availability only. */
export interface NativeCatalogTool {
  name: string;
  available: boolean;
}

/** The runtime catalog: runtime truth, no agent-facing presentation. */
export interface NativeCatalog {
  /** Enforcement contract fingerprint. */
  fingerprint: string;
  tools: NativeCatalogTool[];
  /** Language labels whose LSP server resolves on this machine. */
  lspServers?: string[];
  /** Config `mcp.*` switches: available tools served only through the deferred-tool dispatcher. */
  presentation?: {
    deferred?: string[];
  };
}

/** Startup check of the clasify provider (`probeClassification`). */
export interface ClassificationProbe {
  probed: boolean;
  available: boolean;
  code?: string;
  message?: string;
}

/**
 * Per-request options; the runtime rejects unknown keys.
 * A supplied `githubToken` is the request's only GitHub credential (no env,
 * stored, or `gh` fallback) and is never echoed. Without it the request
 * resolves credentials as before. Hosts serving many users always pass it.
 */
export interface NativeRequestOptions {
  githubToken?: string;
}

export declare class NativeRuntime {
  constructor(options?: NativeRuntimeOptions | null);
  readonly abiVersion: number;
  readonly closed: boolean;
  catalog(): NativeCatalog;
  execute(
    requestId: string,
    tool: string,
    input: unknown,
    options?: NativeRequestOptions | null
  ): Promise<unknown>;
  executeMcp(
    requestId: string,
    tool: string,
    input: unknown,
    options?: NativeRequestOptions | null
  ): Promise<unknown>;
  cancel(requestId: string): boolean;
  /** Startup clasify check; a failure makes clasify unavailable in `catalog()`. */
  probeClassification(): Promise<ClassificationProbe>;
  close(): Promise<void>;
}

declare const runtimeBinding: {
  NativeRuntime: typeof NativeRuntime;
  NATIVE_ABI_VERSION: number;
};

export default runtimeBinding;
