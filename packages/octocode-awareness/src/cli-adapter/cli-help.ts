import { commandSchemaProperties } from '../schema/command-properties.js';
import { AWARENESS_AGENT_INSTRUCTION_SECTIONS, getAwarenessAgentInstructions } from '../agent-instructions.js';
import {
  AWARENESS_CONCEPTS,
  getAwarenessOperationDescriptor,
  listAwarenessOperationDescriptors,
} from '../schema/operation-catalog.js';
import { HELP, HELP_COMPACT } from './cli-help-data.js';
import { extractGlobalDb, normalizeToken } from './cli-routing.js';
import { historyRequestSchemas } from '../schema/definitions-history.js';
import { projectCommandInput } from '../schema/command-input.js';
import {
  maintenanceRetentionDescriptor,
  maintenanceRetentionSchema,
  storeRetirementDescriptor,
  storeRetirementSchema,
} from '../schema/definitions-maintenance.js';

export function hyphenFlag(flag: string): string {
  return `--${flag.replace(/_/g, '-')}`;
}

export function helpFor(command: string | null, options: { compact?: boolean; routeKey?: string } = {}): string {
  if (!command && options.routeKey?.startsWith('noun:')) {
    const noun = options.routeKey.slice('noun:'.length);
    if (noun === 'schema') {
      return [
        'usage: npx @octocodeai/octocode-awareness schema commands|command|entities [options]',
        'commands: print all routine operations',
        'command: print one exact operation contract',
        'entities: print the canonical storage entity catalog',
      ].join('\n');
    }
    if (noun === 'instructions') {
      return [
        'usage: npx @octocodeai/octocode-awareness instructions [--section <name>] [--compact]',
        `sections: ${AWARENESS_AGENT_INSTRUCTION_SECTIONS.join('|')}; repeat --section to compose sections`,
        'default: all sections; same content as getAwarenessAgentInstructions from @octocodeai/octocode-awareness',
      ].join('\n');
    }
    if (noun === 'view') {
      return [
        'usage: npx @octocodeai/octocode-awareness view [options]',
        'flags: --workspace <path> --db <path> --db-scope repo|global --out <file> --no-open --compact',
        'output: private self-contained HTML snapshot of every SQLite entity plus workspace LocalGit status',
        'scope: SQLite rows are store-wide; LocalGit status is workspace-scoped; captured file bytes are not embedded',
      ].join('\n');
    }
    if (noun === 'history-evidence') {
      const properties = commandSchemaProperties(projectCommandInput('history evidence', historyRequestSchemas.history_evidence));
      const flags = [...Object.keys(properties), 'db', 'db_scope', 'compact', 'help'].map(hyphenFlag);
      return [
        'usage: npx @octocodeai/octocode-awareness history evidence [options]',
        `flags: ${flags.join(' ')}`,
        'modes: --action report is the default dry run; --action reclaim requires --confirm reclaim',
        'scope: operator-only LocalGit loose-object maintenance; not a routine agent/Pi operation',
      ].join('\n');
    }
    if (noun === 'maintenance-retention') {
      const properties = commandSchemaProperties(projectCommandInput(
        maintenanceRetentionDescriptor.command,
        maintenanceRetentionSchema,
      ));
      const flags = [...Object.keys(properties), 'workspace', 'db', 'db_scope', 'compact', 'help'].map(hyphenFlag);
      return [
        'usage: npx @octocodeai/octocode-awareness maintenance retention [options]',
        `flags: ${flags.join(' ')}`,
        'modes: --action report is the default dry run; --action apply requires --confirm apply-retention',
        'scope: operator-only lifecycle maintenance; not a routine agent/Pi operation',
      ].join('\n');
    }
    if (noun === 'maintenance-store-retire') {
      const properties = commandSchemaProperties(projectCommandInput(
        storeRetirementDescriptor.command,
        storeRetirementSchema,
      ));
      const flags = [...Object.keys(properties), 'workspace', 'db', 'db_scope', 'compact', 'help'].map(hyphenFlag);
      return [
        'usage: npx @octocodeai/octocode-awareness maintenance store-retire [options]',
        `flags: ${flags.join(' ')}`,
        'modes: save the default report; --action apply requires --confirm retire and --report-file <reviewed-json>',
        'effect: recoverably quarantines the exact SQLite and LocalGit store after lifecycle and writer checks',
      ].join('\n');
    }
    if ((AWARENESS_CONCEPTS as readonly string[]).includes(noun)) {
      const actions = listAwarenessOperationDescriptors()
        .filter(row => row.concept === noun)
        .map(row => row.operation.slice(noun.length + 1));
      return [
        `usage: npx @octocodeai/octocode-awareness ${noun} ${actions.join('|')} [options]`,
        `schema: npx @octocodeai/octocode-awareness schema command ${noun} <operation> --compact`,
      ].join('\n');
    }
  }
  if (!command) return options.compact ? HELP_COMPACT : `${HELP}\n\n${getAwarenessAgentInstructions({ sections: ['start'] })}`;
  const operation = getAwarenessOperationDescriptor(command.includes('.') ? command : command.trim().replace(/\s+/, '.'));
  if (!operation) return options.compact ? HELP_COMPACT : HELP;
  const display = operation.operation.replace('.', ' ');
  const flags = [...new Set([
    ...Object.keys(commandSchemaProperties(operation.inputSchema)),
    'workspace', 'agent_id', 'session_id', 'db', 'db_scope', 'compact', 'help',
  ])].map(hyphenFlag);
  return [
    `usage: npx @octocodeai/octocode-awareness ${display} [options]`,
    `flags: ${flags.join(' ')}`,
    `effect: ${operation.effects.join('|')}`,
  ].join('\n');
}

export function commandFromHelpArgv(argv: string[]): { command: string | null; routeKey?: string } {
  const filtered = extractGlobalDb(argv.filter(arg => !['--help', '-h', '--compact'].includes(arg))).filtered;
  const first = normalizeToken(filtered[0]);
  const second = normalizeToken(filtered[1]);
  if (!first) return { command: null };
  if (first === 'schema') return { command: null, routeKey: 'noun:schema' };
  if (first === 'instructions') return { command: null, routeKey: 'noun:instructions' };
  if (first === 'view') return { command: null, routeKey: 'noun:view' };
  if (first === 'history' && second === 'evidence') return { command: null, routeKey: 'noun:history-evidence' };
  if (first === 'maintenance' && second === 'retention') return { command: null, routeKey: 'noun:maintenance-retention' };
  if (first === 'maintenance' && second === 'store-retire') return { command: null, routeKey: 'noun:maintenance-store-retire' };
  const operation = first && second ? `${first}.${second}` : undefined;
  if (operation && getAwarenessOperationDescriptor(operation)) return { command: operation, routeKey: `${first} ${second}` };
  if ((AWARENESS_CONCEPTS as readonly string[]).includes(first)) return { command: null, routeKey: `noun:${first}` };
  return { command: null };
}
