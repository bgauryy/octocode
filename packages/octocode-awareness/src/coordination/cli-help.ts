import { AWARENESS_COMMANDS, type CommandParamType } from './commands-spec.js';
import { commandIndex } from '../schema/command-catalog.js';

export interface ParsedCoordinationHelpArgs {
  command?: string;
  action?: string;
}

export const COORDINATION_GLOBAL_FLAGS = ['workspace', 'db-scope', 'db', 'compact', 'help'] as const;

export const COORDINATION_CLI_ONLY_ACTION_FLAGS: Readonly<Record<string, Readonly<Record<string, readonly string[]>>>> = {
  guide: { '': ['json'] },
  instructions: { export: ['format'] },
  hooks: { 'pre-edit': ['host', 'agent-id', 'event-json'] },
  memory: {
    'store-verified': ['label', 'text', 'source-digest', 'scope', 'verified-at', 'valid-until', 'importance', 'tags'],
    'recall-verified': ['query', 'label', 'source-digest', 'scope', 'mode', 'limit', 'now', 'min-similarity'],
    evaluate: ['corpus-json', 'now', 'limit', 'min-similarity'],
    reindex: ['force', 'limit'],
    prune: ['older-than', 'label', 'confirm'],
  },
};

function formatFlag(flag: string, type: CommandParamType = 'string', required = false): string {
  const value = type === 'boolean'
    ? `--${flag}`
    : `--${flag} <${type === 'integer' ? 'number' : 'value'}${type === 'string[]' ? '...' : ''}>`;
  return required ? value : `[${value}]`;
}

function globalFlags(): string[] {
  return COORDINATION_GLOBAL_FLAGS.map((flag) => formatFlag(flag, flag === 'compact' || flag === 'help' ? 'boolean' : 'string'));
}

export function focusedCoordinationUsage(parsed: ParsedCoordinationHelpArgs): string | undefined {
  const group = AWARENESS_COMMANDS.find((candidate) => candidate.cli === parsed.command);
  const action = parsed.action ?? '';
  const commandAction = group?.actions.find((candidate) => candidate.action === action)
    ?? (group?.singleton && !action ? group.actions[0] : undefined);
  const cliOnlyFlags = COORDINATION_CLI_ONLY_ACTION_FLAGS[parsed.command ?? '']?.[action];
  const route = [parsed.command, parsed.action]
    .filter((part): part is string => Boolean(part && part !== 'help' && part !== '--help'))
    .join(' ');
  const catalog = commandIndex.find((entry) => entry.command === route);

  if (commandAction || cliOnlyFlags) {
    const flags = [
      ...(commandAction?.params.map((param) => formatFlag(param.flag, param.type, param.required)) ?? []),
      ...(commandAction?.needsAgentId ? [formatFlag(commandAction.agentIdFlag ?? 'agent-id', 'string', true)] : []),
      ...(cliOnlyFlags ?? []).map((flag) => formatFlag(flag)),
      ...globalFlags(),
    ];
    return [
      `usage: npx @octocodeai/octocode-awareness ${route} [options]`,
      commandAction ? `summary: ${commandAction.summary}` : undefined,
      parsed.command === 'hooks' && parsed.action === 'install' ? 'note: hooks install is owned by the root hooks command' : undefined,
      `flags: ${flags.join(' ')}`,
      catalog?.schema ? `schema: ${catalog.schema}` : undefined,
    ].filter((line): line is string => Boolean(line)).join('\n');
  }

  if (group && !parsed.action) {
    return [
      `usage: npx @octocodeai/octocode-awareness ${parsed.command} <${group.actions.map((candidate) => candidate.action).join('|')}> [options]`,
      `actions: ${group.actions.map((candidate) => `${candidate.action} (${candidate.summary})`).join('; ')}`,
      `global flags: ${globalFlags().join(' ')}`,
    ].join('\n');
  }
  return undefined;
}
