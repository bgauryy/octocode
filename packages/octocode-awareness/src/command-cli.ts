import { readFile } from 'node:fs/promises';
import { executeAwarenessCommand, type AwarenessCommandResult } from './command-api.js';
import { getAwarenessCommandDescriptor } from './schema/cli.js';
import { commandIndex } from './schema/command-catalog.js';
import { commandSchemaProperties } from './schema/command-properties.js';
import { parseStorageScope } from './storage-scope.js';
import { parseArgs } from './command-parser.js';
import { extractGlobalDb, validateFlagValues } from './cli-adapter/cli-routing.js';
import { commandFromHelpArgv, helpFor } from './cli-adapter/cli-help.js';
import { AwarenessInputError, commandOutput } from './command-output.js';

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

/** The shell boundary owns tokenization, stdin/file reads, and rendering only. */
export async function executeAwarenessCli(argv: string[], io: { readStdin?: () => Promise<string> } = {}): Promise<AwarenessCommandResult> {
  return commandOutput.run({ command: '', compact: argv.includes('--compact'), text: '', diagnostics: [] }, async () => {
    try {
      if (!argv.length || argv.includes('--help') || argv.includes('-h')) {
        const target = commandFromHelpArgv(argv);
        return { exitCode: 0, payload: null, text: helpFor(target.command, { compact: argv.includes('--compact'), routeKey: target.routeKey }) };
      }
      const globals = extractGlobalDb(argv);
      const booleanValues = new Set(['true', 'false', 'yes', 'no', '0', '1']);
      const tokens = globals.filtered.filter((token, index) => token !== '--compact' || booleanValues.has(globals.filtered[index + 1] ?? ''));
      const parsed = parseArgs(tokens);
      if (tokens.length !== globals.filtered.length) parsed.compact = true;
      validateFlagValues(parsed);
      if (globals.dbPath) parsed.db = globals.dbPath;
      if (globals.dbScope) parsed.db_scope = globals.dbScope;
      const words = parsed._;
      const route = [...commandIndex].sort((a, b) => b.command.length - a.command.length)
        .find(entry => entry.command.split(' ').every((word, i) => words[i] === word));
      const descriptor = route ? getAwarenessCommandDescriptor(route.command) : undefined;
      if (!descriptor) {
        if (words[0] === 'coordination') throw new AwarenessInputError('The coordination prefix was removed.', {
          error_code: 'REMOVED_COORDINATION_ROUTE', hint: `Use ${words.slice(1, 3).join(' ')} through the root command catalog.`,
        });
        const replacement = ({ message: 'signal', check: 'verify' } as Record<string, string>)[words[0] ?? ''];
        if (replacement) throw new AwarenessInputError(`The ${words[0]} command was removed.`, {
          error_code: 'REMOVED_CLI_ROUTE', hint: `Use schema commands and the ${replacement} routes.`,
        });
        throw new Error(`unknown command: ${words.slice(0, 2).join(' ')}`);
      }
      const { _: _words, db, db_scope, compact, ...params } = parsed;
      if (['database consolidate', 'hook run'].includes(descriptor.command) && (db !== undefined || db_scope !== undefined)) {
        throw new Error(`${descriptor.command} does not accept --db or --db-scope`);
      }
      const positionals = words.slice(descriptor.command.split(' ').length);
      if (descriptor.command === 'query' && positionals.length) params.view = positionals.shift()!;
      for (const [index, field] of (descriptor.positionals ?? []).entries()) {
        if (positionals[index] !== undefined) params[field] = positionals[index]!;
      }
      if (positionals.length > (descriptor.positionals?.length ?? 0)) throw new Error('unexpected positional arguments');
      const properties = commandSchemaProperties(descriptor.inputSchema);
      const input = Object.fromEntries(Object.entries(params).map(([key, value]) => [key, coerce(value, properties[key] ?? {})]));
      if (descriptor.command === 'agent register') {
        for (const [field, env] of Object.entries({ agent_name: 'OCTOCODE_AGENT_NAME', agent_vendor: 'OCTOCODE_AGENT_VENDOR', agent_host: 'OCTOCODE_AGENT_HOST' })) {
          if (input[field] === undefined && process.env[env]) input[field] = process.env[env];
        }
      }
      const readInput = async (input: string): Promise<string> => {
        if (input !== '-') return readFile(input, 'utf8');
        if (!io.readStdin) throw new Error('stdin input is unavailable');
        return io.readStdin();
      };
      if (descriptor.stdinField) {
        if (!io.readStdin) throw new Error('hook payload is unavailable');
        input[descriptor.stdinField] = JSON.parse(await io.readStdin());
      }
      if (descriptor.command === 'hooks pre-edit' && input.event_json === undefined) input.event_json = await readInput('-');
      if (db !== undefined && typeof db !== 'string') throw new Error('--db expects a path');
      return executeAwarenessCommand({ command: descriptor.command, params: input }, {
        database: db as string | undefined,
        scope: db_scope === undefined ? undefined : parseStorageScope(String(db_scope)),
        agentId: typeof input.agent_id === 'string' ? input.agent_id : typeof input.lead_agent_id === 'string' ? input.lead_agent_id : process.env.OCTOCODE_AGENT_ID?.trim() || undefined,
        continuations: 'cli',
        compact: compact === true || process.env.OCTOCODE_AWARENESS_COMPACT === '1',
        readInput,
      });
    } catch (error) {
      return { exitCode: 1, payload: { ok: false, error: error instanceof Error ? error.message : String(error), ...(error instanceof AwarenessInputError ? error.details : {}) } };
    }
  });
}
