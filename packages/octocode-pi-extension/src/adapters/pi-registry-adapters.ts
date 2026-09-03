import {
  CommandRegistry,
  ToolRegistry,
  createEffectSet,
  sessionId,
  toolCallId,
  type CommandContext,
  type CommandDefinition as CoreCommandDefinition,
  type CommandResult,
  type ExecutionContext,
  type ToolDefinition as CoreToolDefinition,
  type ToolExecutionInput,
  type ToolResult,
} from '@octocodeai/agent-core';
import type {
  CommandDefinition,
  PiCommandContext,
  PiContext,
  PiInstance,
  ToolCallResult,
  ToolDefinition,
} from '../types.js';

function mode(ctx: PiContext | undefined): ExecutionContext['mode'] {
  return ctx?.mode === 'tui' ? 'interactive' : ctx?.mode ?? 'headless';
}

async function executionContext(ctx: PiContext | undefined, signal: AbortSignal): Promise<ExecutionContext> {
  let trusted: boolean | undefined;
  try { trusted = ctx?.isProjectTrusted ? await ctx.isProjectTrusted() : undefined; } catch { trusted = undefined; }
  const cwd = ctx?.cwd ?? process.cwd();
  const workspace: ExecutionContext['trust']['workspace'] = trusted === undefined ? 'unknown' : trusted ? 'trusted' : 'untrusted';
  const context: ExecutionContext = {
    sessionId: sessionId(ctx?.sessionManager?.getSessionId?.() ?? ctx?.sessionManager?.getSessionFile?.() ?? `pi:${cwd}`),
    cwd,
    mode: mode(ctx),
    trust: { workspace, managedOnly: false },
    signal,
  };
  return Object.freeze(context);
}

function piContent(result: ToolResult): ToolCallResult {
  const content = Array.isArray(result.content)
    ? result.content
    : [{ type: 'text' as const, text: typeof result.content === 'string' ? result.content : JSON.stringify(result.content) }];
  return { content: content as ToolCallResult['content'], isError: !result.ok, details: { version: result.detailsVersion, category: result.category } };
}

export interface PiRegistryRegistrationReceipt {
  readonly kind: 'tool' | 'command';
  readonly name: string;
  readonly owner: string;
  readonly canonicalRegistered: true;
  readonly hostRegistered: true;
}

interface PiToolHostCall {
  readonly callId: string;
  readonly params: Record<string, unknown>;
  readonly signal: AbortSignal;
  readonly onUpdate: unknown;
  readonly context: PiContext | undefined;
}

interface ProjectedToolDispatchResult {
  readonly canonicalResult: ToolResult;
  readonly piResult: ToolCallResult;
}

export type CanonicalToolDispatch = (
  name: string,
  input: ToolExecutionInput,
  hostCall?: PiToolHostCall,
) => Promise<ToolResult | ProjectedToolDispatchResult>;

function nonEmpty(value: unknown, field: string): asserts value is string {
  if (typeof value !== 'string' || value.trim().length === 0) {
    throw new Error(`Invalid canonical registration ${field}`);
  }
}

function stageToolRegistration(definition: CoreToolDefinition, owner: string): void {
  nonEmpty(owner, 'owner');
  nonEmpty(definition.name, 'tool name');
  nonEmpty(definition.label, 'tool label');
  nonEmpty(definition.description, 'tool description');
  if (!Number.isSafeInteger(definition.schemaVersion) || definition.schemaVersion < 1) {
    throw new Error('Invalid canonical registration tool schemaVersion');
  }
  if (!Number.isSafeInteger(definition.outputVersion) || definition.outputVersion < 1) {
    throw new Error('Invalid canonical registration tool outputVersion');
  }
  if (typeof definition.execute !== 'function') {
    throw new Error('Invalid canonical registration tool execute');
  }
  const staging = new ToolRegistry();
  staging.register(definition, owner);
  if (!staging.get(definition.name)) throw new Error(`Failed to stage canonical tool ${definition.name}`);
}

function stageCommandRegistration(definition: CoreCommandDefinition, owner: string): void {
  nonEmpty(owner, 'owner');
  nonEmpty(definition.name, 'command name');
  nonEmpty(definition.description, 'command description');
  if (typeof definition.execute !== 'function') {
    throw new Error('Invalid canonical registration command execute');
  }
  const staging = new CommandRegistry();
  staging.register(definition, owner);
  if (!staging.get(definition.name)) throw new Error(`Failed to stage canonical command ${definition.name}`);
}

export class PiToolRegistryAdapter {
  readonly #receipts: PiRegistryRegistrationReceipt[] = [];

  constructor(
    private readonly pi: PiInstance,
    private readonly registry: ToolRegistry,
    private readonly dispatch: CanonicalToolDispatch,
  ) {}

  listReceipts(): readonly PiRegistryRegistrationReceipt[] {
    return Object.freeze([...this.#receipts]);
  }

  register(definition: CoreToolDefinition, owner: string, projection?: ToolDefinition): void {
    if (this.registry.get(definition.name)) {
      throw new Error(`Duplicate registry identity: ${definition.name}`);
    }
    stageToolRegistration(definition, owner);
    const registerTool = this.pi.registerTool;
    if (typeof registerTool !== 'function') {
      throw new Error(`Pi host cannot register canonical tool ${definition.name}: registerTool is unavailable`);
    }
    const hostDefinition: ToolDefinition = {
      ...projection,
      name: definition.name,
      label: definition.label,
      description: definition.description,
      parameters: definition.inputSchema,
      execute: async (callId, params, signal, onUpdate, ctx) => {
        const controller = signal ? undefined : new AbortController();
        const activeSignal = signal ?? controller!.signal;
        const input: ToolExecutionInput = {
          input: params,
          callId: toolCallId(callId),
          context: await executionContext(ctx, activeSignal),
          signal: activeSignal,
          update: async (update) => {
            if (typeof onUpdate === 'function') await onUpdate(update);
          },
        };
        const dispatched = await this.dispatch(definition.name, input, {
          callId,
          params,
          signal: activeSignal,
          onUpdate,
          context: ctx,
        });
        return 'piResult' in dispatched ? dispatched.piResult : piContent(dispatched);
      },
    };
    registerTool.call(this.pi, hostDefinition);
    this.registry.register(definition, owner);
    this.#receipts.push(Object.freeze({
      kind: 'tool',
      name: definition.name,
      owner,
      canonicalRegistered: true,
      hostRegistered: true,
    }));
  }
}

const COMMAND_CAPABILITIES = new Set<CommandContext['capabilities'] extends ReadonlySet<infer T> ? T : never>([
  'session.read', 'session.mutate', 'model.select', 'settings.read', 'ui.interact',
]);

export type CanonicalCommandDispatch = (
  name: string,
  args: readonly string[],
  context: CommandContext,
  hostContext?: PiCommandContext,
  rawArgs?: string,
) => Promise<CommandResult>;

export class PiCommandRegistryAdapter {
  readonly #receipts: PiRegistryRegistrationReceipt[] = [];

  constructor(
    private readonly pi: PiInstance,
    private readonly registry: CommandRegistry,
    private readonly dispatch: CanonicalCommandDispatch,
  ) {}

  listReceipts(): readonly PiRegistryRegistrationReceipt[] {
    return Object.freeze([...this.#receipts]);
  }

  register(definition: CoreCommandDefinition, owner: string, projection?: CommandDefinition): void {
    if (this.registry.get(definition.name)) {
      throw new Error(`Duplicate registry identity: ${definition.name}`);
    }
    stageCommandRegistration(definition, owner);
    const registerCommand = this.pi.registerCommand;
    if (typeof registerCommand !== 'function') {
      throw new Error(`Pi host cannot register canonical command ${definition.name}: registerCommand is unavailable`);
    }
    const hostDefinition: CommandDefinition = {
      ...projection,
      description: definition.description,
      handler: async (rawArgs: string, ctx: PiCommandContext) => {
        const signal = new AbortController().signal;
        const base = await executionContext(ctx, signal);
        const context: CommandContext = Object.freeze({ ...base, capabilities: COMMAND_CAPABILITIES });
        await this.dispatch(
          definition.name,
          rawArgs.trim() ? rawArgs.trim().split(/\s+/) : [],
          context,
          ctx,
          rawArgs,
        );
      },
      ...(projection?.getArgumentCompletions
        ? { getArgumentCompletions: projection.getArgumentCompletions }
        : definition.complete ? {
        getArgumentCompletions: async (prefix: string) => {
          const signal = new AbortController().signal;
          const context: CommandContext = Object.freeze({
            ...await executionContext(undefined, signal),
            capabilities: COMMAND_CAPABILITIES,
          });
          return (await definition.complete!(prefix, context)).map((value) => ({ value, label: value }));
        },
      } : {}),
    };
    registerCommand.call(this.pi, definition.name, hostDefinition);
    this.registry.register(definition, owner);
    this.#receipts.push(Object.freeze({
      kind: 'command',
      name: definition.name,
      owner,
      canonicalRegistered: true,
      hostRegistered: true,
    }));
  }
}

export interface PiCanonicalRegistryComposition {
  readonly pi: PiInstance;
  readonly toolRegistry: ToolRegistry;
  readonly commandRegistry: CommandRegistry;
  listReceipts(): readonly PiRegistryRegistrationReceipt[];
}

const compositions = new WeakMap<object, PiCanonicalRegistryComposition>();

function canonicalToolResult(result: ToolCallResult): ToolResult {
  return {
    ok: result.isError !== true,
    content: result.content,
    detailsVersion: 1,
  };
}

function toolContract(projection: ToolDefinition): CoreToolDefinition {
  return {
    name: projection.name,
    label: projection.label,
    description: projection.description,
    schemaVersion: 1,
    inputSchema: projection.parameters as CoreToolDefinition['inputSchema'],
    outputSchema: {},
    outputVersion: 1,
    // This compatibility composition establishes canonical ownership and receipts.
    // Use a conservative upper bound: the Pi tool's existing input-sensitive policy
    // remains authoritative until each shared definition supplies exact metadata.
    policy: {
      effects: createEffectSet('read', 'network', 'process', 'write', 'destructive'),
      trust: 'workspace',
      approval: 'always',
      plan: 'allowed',
    },
    execute: async (input) => canonicalToolResult(await projection.execute(
      String(input.callId),
      (input.input ?? {}) as Record<string, unknown>,
      input.signal,
      input.update,
    )),
  };
}

function commandContract(name: string, projection: CommandDefinition): CoreCommandDefinition {
  return {
    name,
    description: projection.description,
    permission: 'approval',
    headless: 'unsupported',
    ...(projection.getArgumentCompletions ? {
      complete: async (prefix: string) => (await projection.getArgumentCompletions!(prefix) ?? [])
        .map((item) => item.value),
    } : {}),
    execute: async () => ({
      status: 'unsupported',
      message: 'This command requires the Pi host command context',
    }),
  };
}

/**
 * Wraps the production Pi instance so every tool and command registration passes
 * through the canonical registries before it becomes part of the public surface.
 * All non-registration members stay bound to the real Pi instance.
 */
export function createPiCanonicalRegistryComposition(
  pi: PiInstance,
  owner = 'builtin:octocode-pi',
): PiCanonicalRegistryComposition {
  const existing = compositions.get(pi as object);
  if (existing) return existing;

  const toolRegistry = new ToolRegistry();
  const commandRegistry = new CommandRegistry();
  const toolProjections = new Map<string, ToolDefinition>();
  const commandProjections = new Map<string, CommandDefinition>();
  const toolAdapter = new PiToolRegistryAdapter(pi, toolRegistry, async (name, input, hostCall) => {
    const projection = toolProjections.get(name);
    if (!projection) throw new Error(`Missing Pi tool projection: ${name}`);
    const piResult = await projection.execute(
      hostCall?.callId ?? String(input.callId),
      hostCall?.params ?? (input.input as Record<string, unknown>),
      hostCall?.signal ?? input.signal,
      hostCall?.onUpdate ?? input.update,
      hostCall?.context,
    );
    return { canonicalResult: canonicalToolResult(piResult), piResult };
  });
  const commandAdapter = new PiCommandRegistryAdapter(
    pi,
    commandRegistry,
    async (name, _args, _context, hostContext, rawArgs) => {
      const projection = commandProjections.get(name);
      if (!projection) throw new Error(`Missing Pi command projection: ${name}`);
      await projection.handler(rawArgs ?? '', hostContext as PiCommandContext);
      return { status: 'ok' };
    },
  );

  let composedPi!: PiInstance;
  const boundMembers = new Map<PropertyKey, unknown>();
  composedPi = new Proxy(pi, {
    get(target, property) {
      if (property === 'registerTool') {
        if (typeof target.registerTool !== 'function') return undefined;
        return (projection: ToolDefinition): void => {
          toolProjections.set(projection.name, projection);
          try {
            toolAdapter.register(toolContract(projection), owner, projection);
          } catch (error) {
            toolProjections.delete(projection.name);
            throw error;
          }
        };
      }
      if (property === 'registerCommand') {
        if (typeof target.registerCommand !== 'function') return undefined;
        return (name: string, projection: CommandDefinition): void => {
          commandProjections.set(name, projection);
          try {
            commandAdapter.register(commandContract(name, projection), owner, projection);
          } catch (error) {
            commandProjections.delete(name);
            throw error;
          }
        };
      }
      const value = Reflect.get(target, property, target);
      if (typeof value !== 'function') return value;
      if (!boundMembers.has(property)) boundMembers.set(property, value.bind(target));
      return boundMembers.get(property);
    },
    set(target, property, value) {
      return Reflect.set(target, property, value, target);
    },
  });

  const composition: PiCanonicalRegistryComposition = Object.freeze({
    pi: composedPi,
    toolRegistry,
    commandRegistry,
    listReceipts: () => Object.freeze([
      ...commandAdapter.listReceipts(),
      ...toolAdapter.listReceipts(),
    ].sort((left, right) => {
      if (left.kind !== right.kind) return left.kind < right.kind ? -1 : 1;
      return left.name === right.name ? 0 : left.name < right.name ? -1 : 1;
    })),
  });
  compositions.set(pi as object, composition);
  compositions.set(composedPi as object, composition);
  return composition;
}

export function getPiRegistryRegistrationReceipts(
  pi: PiInstance,
): readonly PiRegistryRegistrationReceipt[] {
  return compositions.get(pi as object)?.listReceipts() ?? Object.freeze([]);
}
