/**
 * Direct-tool EXECUTION path (P3). This module imports the engine (native LSP
 * client pool) and every tool's execution function via `ALL_TOOLS`, so it is the
 * one that loads the native `.node` addon at eval. It is reached only when a tool
 * actually runs — schema/help/`--scheme`/`context` use `directToolCatalog.meta.ts`
 * (and the `@octocodeai/octocode-tools-core/schema` subpath), which is engine-free.
 */
import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { initialize } from '../serverConfig.js';
import { initializeProviders } from '../providers/factory.js';
import { getConfigSync } from '@octocodeai/config';
import type { ToolConfig } from './toolConfig.js';
import { ALL_TOOLS } from './toolConfig.js';
import {
  buildToolErrorResult,
  sanitizeCallToolResult,
} from '../utils/response/callToolResult.js';
import {
  withBasicSecurityValidation,
  withSecurityValidation,
} from '../security/bridge.js';
import {
  DirectToolInputError,
  type DirectToolDefinition,
  type DirectToolInput,
} from './directToolCatalog.meta.js';

type DirectToolRuntimeDefinition = DirectToolDefinition & {
  execute: (input: DirectToolInput) => Promise<CallToolResult>;
  security: ToolConfig['direct']['security'];
  isLocal: boolean;
  isClone?: boolean;
  requiresServerRuntime?: boolean;
  requiresProviders?: boolean;
};

let serverRuntimeInitPromise: Promise<void> | null = null;
let providerRuntimeInitPromise: Promise<void> | null = null;

// ---------------------------------------------------------------------------
// Test hooks (prefixed with _ per project convention — not part of public API)
// ---------------------------------------------------------------------------
type InitializeFn = () => Promise<void>;
let _initialize: InitializeFn = initialize;

/** @internal — test use only; not part of the public API. */
export function _overrideInitialize(fn: InitializeFn): void {
  if (process.env.NODE_ENV === 'production') {
    throw new Error('_overrideInitialize is not available in production');
  }
  _initialize = fn;
}

/** @internal — test use only; not part of the public API. */
export function _resetInitialize(): void {
  if (process.env.NODE_ENV === 'production') {
    throw new Error('_resetInitialize is not available in production');
  }
  _initialize = initialize;
  serverRuntimeInitPromise = null;
  providerRuntimeInitPromise = null;
}

function wrapExecution(
  fn: ToolConfig['direct']['executionFn']
): (input: DirectToolInput) => Promise<CallToolResult> {
  // executionFn is typed as (input: never) so any specific tool function can be
  // assigned to it (contravariance). At the call site, the input has already been
  // parsed and validated by Zod's inputSchema — this cast reflects that invariant.
  const typedFn = fn as unknown as (
    input: DirectToolInput
  ) => Promise<CallToolResult>;
  return input => typedFn(input);
}

function createDirectTool(tool: ToolConfig): DirectToolRuntimeDefinition {
  const { direct } = tool;
  return {
    name: tool.name,
    schema: direct.schema,
    inputSchema: direct.inputSchema,
    execute: wrapExecution(direct.executionFn),
    security: direct.security,
    isLocal: tool.isLocal,
    isClone: tool.isClone,
    requiresServerRuntime: direct.requiresServerRuntime,
    requiresProviders: direct.requiresProviders,
  };
}

const DIRECT_TOOL_RUNTIME_DEFINITIONS: DirectToolRuntimeDefinition[] =
  ALL_TOOLS.map(createDirectTool);

function findDirectToolRuntimeDefinition(
  name: string
): DirectToolRuntimeDefinition | undefined {
  return DIRECT_TOOL_RUNTIME_DEFINITIONS.find(tool => tool.name === name);
}

export async function executeDirectTool(
  name: string,
  input: unknown
): Promise<CallToolResult> {
  const tool = findDirectToolRuntimeDefinition(name);
  if (!tool) {
    throw new Error(`Unknown tool: ${name}`);
  }

  // LSP client cleanup is handled by the pool's idle timer (idleTimeoutMs,
  // unref()'d). Do not call releaseAllPooledClients() here — it would tear down
  // clients shared by concurrent LSP tool invocations in MCP server mode.
  try {
    const parsedInput = parseDirectToolInput(tool, input);
    await ensureDirectToolRuntimeReady(tool);
    assertDirectToolEnabled(tool);
    return await runDirectTool(tool, parsedInput);
  } catch (error) {
    // Input parsing and runtime readiness can throw; convert to the same
    // structured error envelope as execution failures so non-CLI consumers
    // get a consistent result shape instead of an exception.
    return buildToolErrorResult(tool.name, error);
  }
}

function parseDirectToolInput(
  tool: DirectToolRuntimeDefinition,
  input: unknown
): DirectToolInput {
  const result = tool.inputSchema.safeParse(input);
  if (!result.success) {
    throw result.error;
  }

  return result.data as DirectToolInput;
}

async function ensureDirectToolRuntimeReady(
  tool: DirectToolRuntimeDefinition
): Promise<void> {
  if (tool.requiresServerRuntime) {
    if (!serverRuntimeInitPromise) {
      // Self-heal: clear the cached promise on rejection so the next call
      // retries instead of re-awaiting a stale rejected promise.
      serverRuntimeInitPromise = _initialize().catch(err => {
        serverRuntimeInitPromise = null;
        throw err;
      });
    }
    await serverRuntimeInitPromise;
  }

  if (tool.requiresProviders) {
    if (!providerRuntimeInitPromise) {
      providerRuntimeInitPromise = initializeProviders()
        .then(() => undefined)
        .catch(err => {
          providerRuntimeInitPromise = null;
          throw err;
        });
    }
    await providerRuntimeInitPromise;
  }
}

function assertDirectToolEnabled(tool: DirectToolRuntimeDefinition): void {
  if (!tool.isLocal && !tool.isClone) {
    return;
  }

  const config = getConfigSync();
  if (tool.isLocal && !config.local.enabled) {
    const error = new Error(
      `Tool "${tool.name}" requires local tools. Set ENABLE_LOCAL=true to use it.`
    );
    (error as { code?: string }).code = 'localToolsDisabled';
    throw error;
  }

  // Clone gating is the responsibility of the MCP package (packages/octocode-mcp).
  // The tools-core implementation is gate-free — the interface layer decides
  // whether to register/expose ghCloneRepo based on ENABLE_CLONE.
}

async function runDirectTool(
  tool: DirectToolRuntimeDefinition,
  input: DirectToolInput
): Promise<CallToolResult> {
  try {
    const result =
      tool.security === 'remote'
        ? await runRemoteDirectTool(tool, input)
        : await runBasicDirectTool(tool, input);
    return sanitizeCallToolResult(result);
  } catch (error) {
    return buildToolErrorResult(tool.name, error);
  }
}

async function runRemoteDirectTool(
  tool: DirectToolRuntimeDefinition,
  input: DirectToolInput
): Promise<CallToolResult> {
  const handler = withSecurityValidation<DirectToolInput>(
    tool.name,
    async (sanitizedArgs, authInfo, sessionId) =>
      tool.execute({ ...sanitizedArgs, authInfo, sessionId })
  );

  return handler(input, {});
}

async function runBasicDirectTool(
  tool: DirectToolRuntimeDefinition,
  input: DirectToolInput
): Promise<CallToolResult> {
  const handler = withBasicSecurityValidation<DirectToolInput>(
    tool.execute,
    tool.name
  );

  return handler(input);
}

// Re-export DirectToolInputError so existing `/direct` consumers that import it
// alongside executeDirectTool keep a single import site.
export { DirectToolInputError };
