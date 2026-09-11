import { createAwarenessClient } from './client.js';
import { commandSchemaProperties } from './schema/command-properties.js';
import { getAwarenessOperationDescriptor, type AwarenessOperationCall } from './schema/operation-catalog.js';
import { runSchemaCommand } from './schema/cli.js';
import { parseStorageScope } from './storage-scope.js';
import { parseArgs } from './command-parser.js';
import { extractGlobalDb, validateFlagValues } from './cli-adapter/cli-routing.js';
import { commandFromHelpArgv, helpFor } from './cli-adapter/cli-help.js';
import { AwarenessInputError, commandOutput } from './command-output.js';
import type { AwarenessOperationResult } from './operation-contracts.js';

const IDENTITY_OPTIONAL_OPERATIONS = new Set([
  'work.list', 'work.show', 'memory.recall',
  'history.status', 'history.timeline', 'history.read', 'history.restore',
]);

function coerce(value: unknown, schema: Record<string, unknown>): unknown {
  const variants = [schema, ...(Array.isArray(schema.anyOf) ? schema.anyOf as Record<string, unknown>[] : [])];
  if (Array.isArray(value)) {
    const array = variants.find(candidate => candidate.type === 'array');
    if (array) return value.map(item => coerce(item, array.items as Record<string, unknown> ?? {}));
    if (value.length === 1) return coerce(value[0], schema);
    return value;
  }
  if (variants.some(candidate => candidate.type === 'array') && !variants.some(candidate => candidate.type === typeof value)) {
    const array = variants.find(candidate => candidate.type === 'array')!;
    return [coerce(value, array.items as Record<string, unknown> ?? {})];
  }
  if (typeof value === 'string') {
    if (variants.some(candidate => candidate.type === 'integer' || candidate.type === 'number') && value.trim()) {
      const number = Number(value);
      if (Number.isFinite(number)) return number;
    }
    if (variants.some(candidate => candidate.type === 'boolean')) {
      if (['true', 'yes', '1'].includes(value.toLowerCase())) return true;
      if (['false', 'no', '0'].includes(value.toLowerCase())) return false;
    }
  }
  return value;
}

/** Shell adapter for the canonical operation surface and its schema introspection. */
export async function executeAwarenessCli(
  argv: string[],
  _io: { readStdin?: () => Promise<string> } = {},
): Promise<AwarenessOperationResult> {
  return commandOutput.run({ command: '', compact: argv.includes('--compact'), text: '', diagnostics: [] }, async () => {
    try {
      if (!argv.length || argv.includes('--help') || argv.includes('-h') || argv.every(arg => arg === '--compact')) {
        const target = commandFromHelpArgv(argv);
        return { exitCode: 0, payload: null, text: helpFor(target.command, { compact: argv.includes('--compact'), routeKey: target.routeKey }) };
      }

      const globals = extractGlobalDb(argv);
      const booleanValues = new Set(['true', 'false', 'yes', 'no', '0', '1']);
      const tokens = globals.filtered.filter((token, index) => token !== '--compact' || booleanValues.has(globals.filtered[index + 1] ?? ''));
      const parsed = parseArgs(tokens);
      if (tokens.length !== globals.filtered.length) parsed.compact = true;
      validateFlagValues(parsed);
      const words = parsed._;

      if (words[0] === 'schema') {
        const action = words[1];
        const expectedPositionals = action === 'command' ? 4 : 2;
        if (words.length > expectedPositionals) throw new AwarenessInputError('unexpected positional arguments');
        const { _: _words, compact, all, ...unexpected } = parsed;
        const unknown = Object.keys(unexpected)[0];
        if (unknown) throw new AwarenessInputError(`Unknown flag --${unknown.replaceAll('_', '-')}`);
        const exitCode = await runSchemaCommand(action, {
          ...(compact === true ? { compact: true } : {}),
          ...(all === true ? { all: true } : {}),
          ...(words[2] ? { concept: words[2] } : {}),
          ...(words[3] ? { operation: words[3] } : {}),
        });
        const output = commandOutput.getStore();
        return {
          exitCode,
          payload: output?.payload ?? null,
          ...(output?.text ? { text: output.text } : {}),
          ...(output?.diagnostics.length ? { diagnostics: output.diagnostics } : {}),
        };
      }

      const canonicalName = words.length >= 2 ? `${words[0]}.${words[1]}` : '';
      const descriptor = getAwarenessOperationDescriptor(canonicalName);
      if (!descriptor || words.length !== 2) {
        throw new AwarenessInputError(`Unknown Awareness operation: ${words.slice(0, 2).join(' ') || '<missing>'}`, {
          error_code: 'UNKNOWN_OPERATION',
          hint: 'Use `schema commands --compact` for the complete surface.',
        });
      }

      const { _: _words, compact: _compact, workspace, agent_id, session_id, ...params } = parsed;
      const properties = commandSchemaProperties(descriptor.inputSchema);
      const unknown = Object.keys(params).find(field => !Object.hasOwn(properties, field));
      if (unknown) throw new AwarenessInputError(`Unknown flag --${unknown.replaceAll('_', '-')}`);
      const input = Object.fromEntries(Object.entries(params).map(([key, value]) => [key, coerce(value, properties[key] ?? {})]));
      const actorId = typeof agent_id === 'string' ? agent_id : process.env.OCTOCODE_AGENT_ID?.trim();
      if (!actorId && !IDENTITY_OPTIONAL_OPERATIONS.has(descriptor.operation)) {
        throw new AwarenessInputError('Awareness operations require --agent-id or OCTOCODE_AGENT_ID');
      }
      return createAwarenessClient({
        database: globals.dbPath ?? undefined,
        workspace: typeof workspace === 'string' ? workspace : process.cwd(),
        scope: globals.dbScope === null ? undefined : parseStorageScope(globals.dbScope),
        agentId: actorId ?? 'anonymous-reader',
        sessionId: typeof session_id === 'string' ? session_id : undefined,
      }).execute({ operation: descriptor.operation, params: input } as AwarenessOperationCall);
    } catch (error) {
      return {
        exitCode: 1,
        payload: {
          ok: false,
          error: error instanceof Error ? error.message : String(error),
          ...(error instanceof AwarenessInputError ? error.details : {}),
        },
      };
    }
  });
}
