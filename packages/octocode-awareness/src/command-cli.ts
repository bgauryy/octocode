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
import { getAwarenessAgentInstructions, type AwarenessAgentInstructionSection } from './agent-instructions.js';
import { coerceFlag } from './cli-adapter/cli-coercion.js';
import { connectDb, resolveDbPath } from './db-runtime.js';
import { createOperatorAwarenessView } from './operator-view.js';
import { historyRequestSchemas } from './schema/definitions-history.js';
import { projectCommandInput } from './schema/command-input.js';
import { runAwarenessHistoryOperation } from './history-api.js';
import { maintenanceRetentionSchema, storeRetirementSchema } from './schema/definitions-maintenance.js';
import { runMaintenanceRetention } from './maintenance-retention.js';
import { applyStoreRetirement, reportStoreRetirement } from './store-retirement.js';
import { normalizeWorkspacePath } from './git.js';
import { resolve } from 'node:path';
import { readFileSync } from 'node:fs';
import type { StoreRetirementReport } from './store-retirement.js';

const IDENTITY_OPTIONAL_OPERATIONS = new Set([
  'work.list', 'work.show', 'memory.recall', 'memory.get', 'memory.revalidate',
  'history.status', 'history.timeline', 'history.read', 'history.restore',
]);

/** Shell adapter for the canonical operation surface and its schema introspection. */
export async function executeAwarenessCli(
  argv: string[],
  io: { readStdin?: () => Promise<string>; openFile?: (path: string) => void | Promise<void> } = {},
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

      if (words[0] === 'instructions') {
        const { _: _words, compact: _compact, section, ...unexpected } = parsed;
        if (words.length !== 1 || Object.keys(unexpected).length) {
          throw new AwarenessInputError('Use instructions [--section <name>] [--compact]');
        }
        const selected = section === undefined ? undefined : (Array.isArray(section) ? section : [section]);
        if (selected?.some(value => typeof value !== 'string')) throw new AwarenessInputError('--section requires a section name');
        return { exitCode: 0, payload: null, text: getAwarenessAgentInstructions({
          ...(selected ? { sections: selected as AwarenessAgentInstructionSection[] } : {}),
        }) };
      }

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

      if (words[0] === 'view') {
        const { _: _words, compact: _compact, workspace, out, open, ...unexpected } = parsed;
        if (words.length !== 1) throw new AwarenessInputError('Use view [--workspace <path>] [--out <file>] [--no-open]');
        const unknown = Object.keys(unexpected)[0];
        if (unknown) throw new AwarenessInputError(`Unknown flag --${unknown.replaceAll('_', '-')}`);
        if (workspace !== undefined && typeof workspace !== 'string') throw new AwarenessInputError('--workspace requires a path');
        if (out !== undefined && typeof out !== 'string') throw new AwarenessInputError('--out requires a path');
        const selectedWorkspace = workspace ?? process.cwd();
        const scope = globals.dbScope === null ? undefined : parseStorageScope(globals.dbScope);
        const payload = await createOperatorAwarenessView({
          database: resolveDbPath(globals.dbPath, { scope, workspace: selectedWorkspace }),
          workspace: selectedWorkspace,
          ...(out ? { out } : {}),
          ...(open === false ? { open: false } : {}),
          ...(io.openFile ? { openFile: io.openFile } : {}),
        });
        return {
          exitCode: 0,
          payload,
          ...(payload.open_error ? { diagnostics: [`Awareness view was generated but could not be opened: ${payload.open_error}`] } : {}),
        };
      }

      if (words[0] === 'history' && words[1] === 'evidence') {
        if (words.length !== 2) throw new AwarenessInputError('Use history evidence [--action report|reclaim] [options]');
        const { _: _words, compact: _compact, workspace, ...params } = parsed;
        const inputSchema = projectCommandInput('history evidence', historyRequestSchemas.history_evidence);
        const properties = commandSchemaProperties(inputSchema);
        const unknown = Object.keys(params).find(field => !Object.hasOwn(properties, field));
        if (unknown) throw new AwarenessInputError(`Unknown flag --${unknown.replaceAll('_', '-')}`);
        const selectedWorkspace = typeof workspace === 'string' ? workspace : process.cwd();
        const input = historyRequestSchemas.history_evidence.parse({
          workspace: selectedWorkspace,
          ...Object.fromEntries(Object.entries(params)
            .map(([key, value]) => [key, coerceFlag(value, properties[key] ?? {})])),
        });
        const scope = globals.dbScope === null ? undefined : parseStorageScope(globals.dbScope);
        const db = connectDb(resolveDbPath(globals.dbPath, { scope, workspace: selectedWorkspace }));
        try {
          return { exitCode: 0, payload: await runAwarenessHistoryOperation(db, 'evidence', input) };
        } finally {
          db.close();
        }
      }

      if (words[0] === 'maintenance' && words[1] === 'retention') {
        if (words.length !== 2) throw new AwarenessInputError('Use maintenance retention [--action report|apply] [options]');
        const { _: _words, compact: _compact, workspace, ...params } = parsed;
        const inputSchema = projectCommandInput('maintenance retention', maintenanceRetentionSchema);
        const properties = commandSchemaProperties(inputSchema);
        const unknown = Object.keys(params).find(field => !Object.hasOwn(properties, field));
        if (unknown) throw new AwarenessInputError(`Unknown flag --${unknown.replaceAll('_', '-')}`);
        const requestedWorkspace = typeof workspace === 'string' ? workspace : process.cwd();
        const selectedWorkspace = normalizeWorkspacePath(requestedWorkspace, requestedWorkspace)
          ?? resolve(requestedWorkspace);
        const input = maintenanceRetentionSchema.parse(Object.fromEntries(Object.entries(params)
          .map(([key, value]) => [key, coerceFlag(value, properties[key] ?? {})])));
        const scope = globals.dbScope === null ? undefined : parseStorageScope(globals.dbScope);
        const database = resolveDbPath(globals.dbPath, { scope, workspace: selectedWorkspace });
        const db = connectDb(database);
        try {
          return { exitCode: 0, payload: runMaintenanceRetention(db, selectedWorkspace, input, { dbPath: database }) };
        } finally {
          db.close();
        }
      }

      if (words[0] === 'maintenance' && words[1] === 'store-retire') {
        if (words.length !== 2) throw new AwarenessInputError('Use maintenance store-retire [--action report|apply] [options]');
        const { _: _words, compact: _compact, workspace, ...params } = parsed;
        const inputSchema = projectCommandInput('maintenance store-retire', storeRetirementSchema);
        const properties = commandSchemaProperties(inputSchema);
        const unknown = Object.keys(params).find(field => !Object.hasOwn(properties, field));
        if (unknown) throw new AwarenessInputError(`Unknown flag --${unknown.replaceAll('_', '-')}`);
        const requestedWorkspace = typeof workspace === 'string' ? workspace : process.cwd();
        const selectedWorkspace = normalizeWorkspacePath(requestedWorkspace, requestedWorkspace)
          ?? resolve(requestedWorkspace);
        const input = storeRetirementSchema.parse(Object.fromEntries(Object.entries(params)
          .map(([key, value]) => [key, coerceFlag(value, properties[key] ?? {})])));
        const scope = globals.dbScope === null ? undefined : parseStorageScope(globals.dbScope);
        const database = resolveDbPath(globals.dbPath, { scope, workspace: selectedWorkspace });
        if (input.action === 'report') {
          return { exitCode: 0, payload: reportStoreRetirement({ database, workspaces: [selectedWorkspace] }) };
        }
        if (!input.report_file) throw new AwarenessInputError('--report-file is required for store retirement apply');
        const report = JSON.parse(readFileSync(resolve(input.report_file), 'utf8')) as StoreRetirementReport;
        if (globals.dbPath && resolve(globals.dbPath) !== report.database?.path) {
          throw new AwarenessInputError('--db must match the reviewed retirement report');
        }
        if (typeof workspace === 'string' && !report.workspaces?.includes(selectedWorkspace)) {
          throw new AwarenessInputError('--workspace must be included in the reviewed retirement report');
        }
        return { exitCode: 0, payload: applyStoreRetirement({ report, confirm: input.confirm }) };
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
      const input = Object.fromEntries(Object.entries(params).map(([key, value]) => [key, coerceFlag(value, properties[key] ?? {})]));
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
