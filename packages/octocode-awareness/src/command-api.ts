import { normalizeWorkspacePath } from './git.js';
import { structuredAwarenessContinuations } from './command-continuations.js';
import { z } from 'zod';
import { resolve } from 'node:path';
import { HistoryError } from './history-store.js';
import { commandOutput, AwarenessInputError, type AwarenessCommandOutput } from './command-output.js';
import { getAwarenessCommandDescriptor, runSchemaCommand } from './schema/cli.js';
import { commandSchemaProperties } from './schema/command-properties.js';
import { connectDb, resolveDbPath } from './db-runtime.js';
import { storageScopeForCommand } from './workspace-policy.js';
import type { AwarenessStorageScope } from './storage-scope.js';
import { COMMAND_ROUTES } from './commands/routes.js';
import { BUNDLED_SKILLS_DIR, packageSkillScriptPath } from './skill-paths.js';
import type { ParsedArgs } from './commands/args.js';
import type { AwarenessCommandCall as AwarenessCommandCallContract } from './command-contracts.js';

export type AwarenessCommandCall = AwarenessCommandCallContract;
export interface AwarenessCommandContext {
  database?: string;
  workspace?: string;
  agentId?: string;
  scope?: AwarenessStorageScope;
  compact?: boolean;
  signal?: AbortSignal;
  /** Shell adapters alone may resolve schema-validation input from file/stdin. */
  readInput?: (input: string) => Promise<string>;
  /** CLI adapters retain shell continuations; library callers receive request objects. */
  continuations?: 'api' | 'cli';
}
export interface AwarenessCommandResult {
  payload: unknown;
  exitCode: number;
  text?: string;
  diagnostics?: string[];
  cancelled?: boolean;
}

const validators = new Map<string, z.ZodType>();

function validate(command: string, params: Record<string, unknown>, schema: Record<string, unknown>): void {
  let validator = validators.get(command);
  if (!validator) {
    validator = z.fromJSONSchema(schema);
    validators.set(command, validator);
  }
  const result = validator.safeParse(params);
  if (!result.success) {
    const issues = result.error.issues.map(issue => ({
      path: issue.path.join('.'),
      message: issue.code === 'unrecognized_keys'
        ? `unknown flag${issue.keys.length > 1 ? 's' : ''}: ${issue.keys.map(key => `--${key.replaceAll('_', '-')}`).join(', ')}`
        : `${issue.path.length ? `--${issue.path.join('.').replaceAll('_', '-')} ` : ''}${issue.message}`,
    }));
    const hint = command === 'task create' && result.error.issues.some(issue => issue.code === 'unrecognized_keys' && issue.keys.some(key => ['run_id', 'lease_minutes', 'test_plan'].includes(key)))
      ? ' Set run options with task claim.' : '';
    throw new AwarenessInputError(`Invalid parameters for ${command}: ${issues.map(issue => issue.message).join('; ')}${hint}`, {
      issues,
      ...(result.error.issues.some(issue => issue.code === 'unrecognized_keys')
        ? { known_flags: Object.keys(commandSchemaProperties(schema)).map(key => `--${key.replaceAll('_', '-')}`) } : {}),
    });
  }
}

/** Normalize values for existing domain handlers; never construct or parse argv. */
function handlerParams(params: Record<string, unknown>): ParsedArgs {
  return Object.fromEntries([['_', []], ...Object.entries(params).map(([key, value]) => [key,
    key.endsWith('_json') && typeof value === 'object' ? JSON.stringify(value)
      : typeof value === 'number' ? String(value)
      : Array.isArray(value) ? value.map(item => typeof item === 'string' ? item : JSON.stringify(item))
        : value && typeof value === 'object' ? JSON.stringify(value) : value,
  ])]) as ParsedArgs;
}

function duration(value: unknown): number | undefined {
  if (value === undefined) return undefined;
  const match = String(value).match(/^(\d+(?:\.\d+)?)(ms|s|m|h|d|w)?$/);
  if (!match) throw new AwarenessInputError('Invalid duration');
  return Number(match[1]) * ({ ms: 1, s: 1000, m: 60000, h: 3600000, d: 86400000, w: 604800000 }[match[2] ?? 'ms'] ?? 1);
}

const HOST_COMMANDS = new Set(['agent touch', 'agent leave', 'handoff add', 'handoff list', 'handoff clear',
  'memory store-verified', 'memory recall-verified', 'memory evaluate', 'memory reindex', 'memory prune']);

/**
 * Complete command API shared by native tools and the CLI. Requests keep their
 * own output/error scope across awaits; importing or calling never exits the
 * host, changes cwd/env, reads stdin, or launches the Awareness binary.
 * Cancellation is cooperative between operations and during lock waits.
 * Completed atomic writes are reported as completed even if cancellation follows.
 */
export async function executeAwarenessCommand(request: AwarenessCommandCall, context: AwarenessCommandContext = {}): Promise<AwarenessCommandResult> {
  const output: AwarenessCommandOutput = { command: request.command, compact: context.compact ?? false, text: '', diagnostics: [] };
  return commandOutput.run(output, async () => {
    try {
      context.signal?.throwIfAborted();
      const descriptor = getAwarenessCommandDescriptor(request.command);
      if (!descriptor) throw new AwarenessInputError(`Unknown Awareness command: ${request.command}`);
      const params = { ...request.params };
      const properties = commandSchemaProperties(descriptor.inputSchema);
      const bind = (key: string, value: string | undefined) => {
        if (value === undefined || !Object.hasOwn(properties, key)) return;
        if (params[key] !== undefined && params[key] !== value && !(key === 'workspace' && typeof params[key] === 'string' && normalizeWorkspacePath(params[key], params[key]) === normalizeWorkspacePath(value, value))) throw new AwarenessInputError(`${key} conflicts with the host binding`);
        params[key] = value;
      };
      bind('workspace', context.workspace);
      if (descriptor.injected.includes('agent-id')) bind(Object.hasOwn(properties, 'agent_id') ? 'agent_id' : 'lead_agent_id', context.agentId);
      validate(request.command, params, descriptor.inputSchema as Record<string, unknown>);
      const workspace = resolve(context.workspace ?? String(params.workspace ?? process.cwd()));
      const [noun, action] = request.command.split(' ');
      const metadataOnly = ['schema', 'config', 'guide', 'instructions', 'skill', 'database'].includes(noun!)
        || request.command === 'maintenance self-test' || (noun === 'docs' && action !== 'staleness');
      const scope = metadataOnly ? context.scope : storageScopeForCommand(COMMAND_ROUTES[request.command]?.command ?? request.command, workspace, context.scope);
      const dbPath = metadataOnly ? context.database ?? '' : resolveDbPath(context.database, { scope, workspace });
          const opts = { compact: output.compact, cli: context.continuations === 'cli' };
      let exitCode = 0;

      if (noun === 'schema') {
        exitCode = await runSchemaCommand(action, { ...params, compact: output.compact }, { readInput: context.readInput });
      } else if (request.command === 'guide' || request.command === 'instructions export') {
        const policy = await import('./coordination/external-policy.js');
        context.signal?.throwIfAborted();
        if (request.command === 'guide') {
          if (params.json) output.payload = policy.getExternalAgentAwarenessGuide();
          else output.text = policy.EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS;
        } else {
          const format = String(params.format ?? 'prompt');
          if (format === 'json') output.payload = { format: 'prompt', instructions: policy.EXTERNAL_AGENT_AWARENESS_PROMPT };
          else output.text = policy.formatExternalAgentAwarenessInstructions(format === 'agents-md' ? 'agents-md' : 'prompt');
        }
      } else if (HOST_COMMANDS.has(request.command)) {
        const { openAwarenessStore } = await import('./coordination/open.js');
        context.signal?.throwIfAborted();
        const { dispatchAwarenessCommand } = await import('./coordination/dispatch.js');
        context.signal?.throwIfAborted();
        const aw = openAwarenessStore({ workspace, dbPath, scope });
        try {
          const p = Object.fromEntries(Object.entries(params).map(([key, value]) => [key.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase()), value]));
          if (noun === 'handoff') p.files = params.file;
          if (action === 'prune') { p.olderThanMs = duration(params.older_than); p.dryRun = params.confirm !== true; }
          const result = dispatchAwarenessCommand(aw, { command: noun!, action, params: p });
          output.payload = result.result;
          exitCode = result.exitCode;
        } finally { aw.close(); }
      } else if (request.command === 'database consolidate') {
        const { consolidateDatabase } = await import('./db-consolidation.js');
        context.signal?.throwIfAborted();
        output.payload = { ok: true, ...consolidateDatabase(String(params.source), String(params.destination), { dryRun: params.dry_run === true }) };
      } else if (noun === 'config') {
        const { cmdAwarenessConfig } = await import('./commands/config.js');
        context.signal?.throwIfAborted();
        exitCode = cmdAwarenessConfig(handlerParams({ ...params, action }), opts);
      } else if (request.command === 'maintenance self-test') {
        const { cmdSelfTest } = await import('./commands/admin.js');
        context.signal?.throwIfAborted();
        exitCode = await cmdSelfTest(opts);
      } else if (request.command === 'skill install') {
        const { runSkillInstall } = await import('./skill-install-command.js');
        context.signal?.throwIfAborted();
        const result = runSkillInstall(params, { skillsDir: BUNDLED_SKILLS_DIR, cwd: workspace });
        output.payload = result.payload; exitCode = result.exitCode;
      } else if (request.command === 'skill list') {
        const { runSkillList } = await import('./skill-list-command.js');
        context.signal?.throwIfAborted();
        const result = runSkillList(params, { skillsDir: BUNDLED_SKILLS_DIR, cwd: workspace });
        output.payload = result.payload; exitCode = result.exitCode;
      } else if (request.command === 'skill check') {
        const { runSkillCheck } = await import('./skill-check-command.js');
        context.signal?.throwIfAborted();
        const result = runSkillCheck(params, { skillsDir: BUNDLED_SKILLS_DIR, cwd: workspace });
        output.payload = result.payload; exitCode = result.exitCode;
      } else if (request.command === 'skill remove') {
        const { runSkillRemove } = await import('./skill-remove-command.js');
        context.signal?.throwIfAborted();
        const result = runSkillRemove(params, { skillsDir: BUNDLED_SKILLS_DIR, cwd: workspace });
        output.payload = result.payload; exitCode = result.exitCode;
      } else if (noun === 'hooks' && action !== 'pre-edit') {
        const { runHooksInstall } = await import('./hooks-install-command.js');
        context.signal?.throwIfAborted();
        const result = runHooksInstall({ ...params, compact: output.compact, check: action === 'check', remove: action === 'remove' }, { hookDir: packageSkillScriptPath('hooks'), dbPath, cwd: workspace });
        output.payload = result.payload; output.text = result.text ?? ''; exitCode = result.exitCode;
      } else if (request.command === 'hooks pre-edit') {
        const { runPreEditLockGate } = await import('./coordination/hooks.js');
        context.signal?.throwIfAborted();
        const event = typeof params.event_json === 'string' ? JSON.parse(params.event_json) : params.event_json ?? {};
        const result = runPreEditLockGate({ workspace, dbPath, scope, agentId: context.agentId ?? String(params.agent_id ?? ''), host: params.host as import('./coordination/hooks.js').HookHost, event });
        output.payload = result; exitCode = result.blocked ? 2 : 0;
      } else if (request.command === 'hook run') {
        if (context.database !== undefined || context.scope !== undefined) throw new AwarenessInputError('hook run uses the payload workspace store; database/scope overrides are not supported');
        if (params.payload === undefined) throw new AwarenessInputError('hook run requires an explicit payload in the library API');
        const { runHookCommand } = await import('./hooks/runner.js');
        context.signal?.throwIfAborted();
        exitCode = await runHookCommand(String(params.event), typeof params.payload === 'string' ? params.payload : JSON.stringify(params.payload));
      } else if (noun === 'docs' && action !== 'staleness') {
        const { cmdDocsCatalog } = await import('./commands/repo.js');
        context.signal?.throwIfAborted();
        exitCode = cmdDocsCatalog(handlerParams({ ...params, action }), opts);
      } else {
        const db = connectDb(dbPath);
        try {
          const route = COMMAND_ROUTES[request.command];
          const args = handlerParams(params);
          if (route?.action) args.action = route.action;
          if (noun === 'query' && action) args.view = action;
          // Scope defaults are execution context, never borrowed from another host request.
          if (Object.hasOwn(properties, 'workspace')) args.workspace ??= workspace;
          if (request.command === 'lock wait' || request.command === 'lock acquire') {
            const { runLockCommand } = await import('./command-locks.js');
            context.signal?.throwIfAborted();
            exitCode = await runLockCommand(db, request.command, args, dbPath, opts, context.signal);
          } else {
            const { runDatabaseCommandHandler } = await import('./command-dispatch.js');
            context.signal?.throwIfAborted();
            exitCode = await runDatabaseCommandHandler(db, route?.command ?? noun!, args, dbPath, opts, context.signal);
          }
        } finally { db.close(); }
      }
      return { payload: context.continuations === 'cli' ? output.payload ?? null : structuredAwarenessContinuations(output.payload ?? null), exitCode, ...(output.text ? { text: output.text } : {}), ...(output.diagnostics.length ? { diagnostics: output.diagnostics } : {}) };
    } catch (error) {
      const cancelled = context.signal?.aborted === true && (error === context.signal.reason || (error instanceof Error && error.name === 'AbortError'));
      return { exitCode: 1, payload: { ok: false, command: request.command, error: error instanceof HistoryError ? { code: error.code, message: error.message } : error instanceof Error ? error.message : String(error), ...(error instanceof AwarenessInputError ? error.details : {}) }, ...(cancelled ? { cancelled: true } : {}) };
    }
  });
}
