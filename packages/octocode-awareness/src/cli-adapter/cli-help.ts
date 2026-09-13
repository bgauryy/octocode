import { commandSchemaProperties } from '../schema/command-properties.js';
import { AWARENESS_AGENT_INSTRUCTION_SECTIONS, getAwarenessAgentInstructions } from '../agent-instructions.js';
import {
  AWARENESS_CONCEPTS,
  getAwarenessOperationDescriptor,
  listAwarenessOperationDescriptors,
} from '../schema/operation-catalog.js';
import { HELP, HELP_COMPACT } from './cli-help-data.js';
import { extractGlobalDb, normalizeToken } from './cli-routing.js';

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
  const operation = first && second ? `${first}.${second}` : undefined;
  if (operation && getAwarenessOperationDescriptor(operation)) return { command: operation, routeKey: `${first} ${second}` };
  if ((AWARENESS_CONCEPTS as readonly string[]).includes(first)) return { command: null, routeKey: `noun:${first}` };
  return { command: null };
}
