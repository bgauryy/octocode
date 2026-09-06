#!/usr/bin/env node
import { readFileSync,realpathSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { AWARENESS_COMMANDS } from './commands-spec.js';
import { dispatchAwarenessCommand,type AwarenessCommandRequest } from './dispatch.js';
import {
  EXTERNAL_AGENT_AWARENESS_PROMPT,
  EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS,
  formatExternalAgentAwarenessInstructions,
  getExternalAgentAwarenessGuide,
} from './external-policy.js';
import { runPreEditLockGate,type HookHost } from './hooks.js';
import { COORDINATION_CLI_ONLY_ACTION_FLAGS, COORDINATION_GLOBAL_FLAGS, focusedCoordinationUsage } from './cli-help.js';
import { openAwarenessStore } from './open.js';
import { parseStorageScope } from '../storage-scope.js';
import { storageScopeForCommand } from '../workspace-policy.js';

interface ParsedArgs {
  command?: string;
  action?: string;
  flags: Map<string, string | true>;
}

function parseArgs(argv: string[]): ParsedArgs {
  const [command, maybeAction, ...tail] = argv;
  const action = maybeAction?.startsWith('--') ? undefined : maybeAction;
  const rest = action ? tail : [maybeAction, ...tail].filter((arg): arg is string => Boolean(arg));
  const flags = new Map<string, string | true>();
  for (let index = 0; index < rest.length; index += 1) {
    const arg = rest[index];
    if (!arg?.startsWith('--')) throw new Error(`unexpected argument: ${arg}`);
    const equalsIndex = arg.indexOf('=');
    const key = equalsIndex >= 0 ? arg.slice(2, equalsIndex) : arg.slice(2);
    if (equalsIndex >= 0) {
      const value = arg.slice(equalsIndex + 1);
      if (!value) throw new Error(`--${key} expects a value`);
      flags.set(key, value);
      continue;
    }
    const next = rest[index + 1];
    if (!next || next.startsWith('--')) {
      flags.set(key, true);
    } else {
      flags.set(key, next);
      index += 1;
    }
  }
  return { command, action, flags };
}

function getFlag(flags: Map<string, string | true>, name: string): string | undefined {
  const value = flags.get(name);
  if (typeof value === 'string') return value;
  return undefined;
}

function requireFlag(flags: Map<string, string | true>, name: string): string {
  const value = getFlag(flags, name);
  if (!value?.trim()) throw new Error(`--${name} is required`);
  return value;
}

function parseDurationMs(value: string): number {
  const match = value.trim().match(/^(\d+(?:\.\d+)?)(ms|s|m|h|d|w)?$/);
  if (!match) throw new Error(`invalid duration: ${value}`);
  const amount = Number(match[1]);
  const unit = match[2] ?? 'ms';
  const multipliers: Record<string, number> = { ms: 1, s: 1000, m: 60_000, h: 3_600_000, d: 86_400_000, w: 604_800_000 };
  return amount * multipliers[unit]!;
}

function getDurationFlag(flags: Map<string, string | true>, name: string): number | undefined {
  const value = getFlag(flags, name);
  return value ? parseDurationMs(value) : undefined;
}

function requireDurationFlag(flags: Map<string, string | true>, name: string): number {
  return parseDurationMs(requireFlag(flags, name));
}

/**
 * Output sink for `print`. Defaults to real stdout so the `bin` behaves exactly
 * as before; `runCli`/`execCli` swap it so an in-process host can capture the same JSON a
 * spawned `cli.js` would have written, without a child process.
 */
let currentWriter: (chunk: string) => void = (chunk) => { process.stdout.write(chunk); };
let compactOutput = false;

function print(value: unknown): void {
  currentWriter(`${JSON.stringify(value, null, compactOutput ? 0 : 2)}\n`);
}

function usage(): string {
  return 'octocode-awareness status | handoff add|list|clear | agent touch|leave | memory store-verified|recall-verified|evaluate|reindex|prune | guide | instructions export | hooks pre-edit';
}
function hasHelpFlag(parsed: ParsedArgs): boolean {
  return parsed.command === 'help' || parsed.command === '--help' || parsed.action === 'help' || parsed.action === '--help' || parsed.flags.has('help');
}

function validateFlags(parsed: ParsedArgs): void {
  if (!parsed.command) return;
  const action = parsed.action ?? '';
  const group = AWARENESS_COMMANDS.find((candidate) => candidate.cli === parsed.command);
  const commandAction = group?.actions.find((candidate) => candidate.action === action)
    ?? (group?.singleton && !action ? group.actions[0] : undefined);
  const cliOnlyFlags = COORDINATION_CLI_ONLY_ACTION_FLAGS[parsed.command]?.[action];
  // Preserve each command's own unknown-command/action error when no canonical
  // action contract exists. Known actions are strict and accept only their
  // declared params, host-injected agent id, CLI-only params, and globals.
  if (!commandAction && !cliOnlyFlags) return;
  const allowed = new Set<string>(COORDINATION_GLOBAL_FLAGS);
  for (const param of commandAction?.params ?? []) allowed.add(param.flag);
  if (commandAction?.needsAgentId) allowed.add(commandAction.agentIdFlag ?? 'agent-id');
  for (const flag of cliOnlyFlags ?? []) allowed.add(flag);
  for (const flag of parsed.flags.keys()) {
    if (!allowed.has(flag)) throw new Error(`unknown flag: --${flag}`);
  }
}

function readJsonInput(flags: Map<string, string | true>): unknown {
  const raw = getFlag(flags, 'event-json') ?? readFileSync(0, 'utf8');
  if (!raw.trim()) return {};
  return JSON.parse(raw) as unknown;
}

function defaultHookAgentId(host: string): string {
  return process.env.OCTOCODE_AGENT_ID || `${host || 'hook'}:${process.pid}`;
}

/**
 * Translate the CLI's parsed flags into the neutral, tool-facing `params` dict
 * that `dispatchAwarenessCommand` consumes. This is the CLI's ONLY awareness
 * responsibility beyond argv parsing: the command→library-method mapping lives
 * once in `dispatch.ts`, so the CLI never names an `AwarenessStore` method or its
 * param shape. Over-reading a flag an action ignores is harmless — the
 * dispatcher only uses the keys each action needs. Duration flags are converted
 * to integer milliseconds here (the parser's job); the dispatcher receives ms.
 */
function buildCommandParams(parsed: ParsedArgs): Record<string, unknown> {
  const f = parsed.flags;
  const s = (name: string) => getFlag(f, name);
  const d = (name: string) => getDurationFlag(f, name);
  const has = (name: string) => f.has(name);
  const isPrune = parsed.action === 'prune';

  switch (parsed.command) {
    case 'status':
      return { staleAfterMs: d('stale-after') };
    case 'handoff':
      return { agentId: s('agent-id'), summary: s('summary'), files: s('file'), includeCleared: has('include-cleared'), handoffId: s('handoff-id') };
    case 'agent':
      return { agentId: s('agent-id'), status: s('status') };
    case 'memory':
      return {
        label: s('label'), text: s('text'), tags: s('tags'), query: s('query'), limit: s('limit'),
        semantic: has('semantic'), minSimilarity: s('min-similarity'),
        sourceDigest: s('source-digest'), scope: s('scope'), verifiedAt: s('verified-at'), validUntil: s('valid-until'),
        importance: s('importance'), mode: s('mode'), now: s('now'), corpusJson: s('corpus-json'),
        force: has('force'), memoryId: s('memory-id'),
        olderThanMs: isPrune ? requireDurationFlag(f, 'older-than') : undefined,
        dryRun: isPrune ? !has('confirm') : undefined,
      };
    default:
      return {};
  }
}

export function runCli(argv: string[], io: { write?: (chunk: string) => void } = {}): number {
  const previousWriter = currentWriter;
  const previousCompact = compactOutput;
  if (io.write) currentWriter = io.write;
  try {
    return runCliInner(argv, io.write ?? previousWriter);
  } finally {
    currentWriter = previousWriter;
    compactOutput = previousCompact;
  }
}

function runCliInner(argv: string[], write: (chunk: string) => void): number {
  const parsed = parseArgs(argv);
  compactOutput = parsed.flags.has('compact') || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1';
  if (!parsed.command) {
    write(`${usage()}\n`);
    return 0;
  }
  if (hasHelpFlag(parsed)) {
    write(`${focusedCoordinationUsage(parsed) ?? usage()}\n`);
    return 0;
  }
  const supported = parsed.command === 'memory'
    ? ['store-verified', 'recall-verified', 'evaluate', 'reindex', 'prune'].includes(parsed.action ?? '')
    : parsed.command === 'agent' ? ['touch', 'leave'].includes(parsed.action ?? '')
    : ['status', 'handoff', 'guide', 'instructions', 'hooks'].includes(parsed.command);
  if (!supported) throw new Error(`command is not owned by this adapter: ${parsed.command}; use the canonical root CLI`);
  validateFlags(parsed);

  if (parsed.command === 'guide') {
    if (parsed.action) throw new Error('guide does not accept an action');
    if (parsed.flags.has('json')) print(getExternalAgentAwarenessGuide());
    else write(`${EXTERNAL_AGENT_AWARENESS_PROMPT}\n`);
    return 0;
  }

  if (parsed.command === 'instructions') {
    if (parsed.action !== 'export') throw new Error('instructions action must be export');
    const format = getFlag(parsed.flags, 'format') ?? 'prompt';
    if (!['prompt', 'agents-md', 'json'].includes(format)) {
      throw new Error('instructions export --format must be prompt, agents-md, or json');
    }
    if (format === 'json') print({ format: 'prompt', instructions: EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS });
    else write(`${formatExternalAgentAwarenessInstructions(format === 'agents-md' ? 'agents-md' : 'prompt')}\n`);
    return 0;
  }

  // `hooks` is host integration (stdin + filesystem), not a coordination
  // command, so it does not route through the shared dispatcher.
  if (parsed.command === 'hooks') {
    switch (parsed.action) {
      case 'install': {
        throw new Error('hooks installation is owned by the root hooks install command');
      }
      case 'pre-edit': {
        const hookWorkspace = getFlag(parsed.flags, 'workspace') ?? process.cwd();
        const result = runPreEditLockGate({
          workspace: hookWorkspace,
          dbPath: getFlag(parsed.flags, 'db'),
          scope: storageScopeForCommand(
            'coordination',
            hookWorkspace,
            getFlag(parsed.flags, 'db-scope') ? parseStorageScope(getFlag(parsed.flags, 'db-scope')) : undefined,
          ),
          agentId: getFlag(parsed.flags, 'agent-id') ?? defaultHookAgentId(getFlag(parsed.flags, 'host') ?? 'generic'),
          host: (getFlag(parsed.flags, 'host') ?? 'generic') as HookHost,
          event: readJsonInput(parsed.flags),
        });
        print(result);
        return result.blocked ? 2 : 0;
      }
      default:
        throw new Error('hooks action must be install or pre-edit');
    }
  }

  const commandWorkspace = getFlag(parsed.flags, 'workspace') ?? process.cwd();
  const aw = openAwarenessStore({
    workspace: commandWorkspace,
    dbPath: getFlag(parsed.flags, 'db'),
    scope: storageScopeForCommand(
      'coordination',
      commandWorkspace,
      getFlag(parsed.flags, 'db-scope') ? parseStorageScope(getFlag(parsed.flags, 'db-scope')) : undefined,
    ),
  });

  try {
    const request: AwarenessCommandRequest = {
      command: parsed.command,
      action: parsed.action,
      params: buildCommandParams(parsed),
    };
    const { result, exitCode } = dispatchAwarenessCommand(aw, request);
    print(result);
    return exitCode;
  } finally {
    aw.close();
  }
}

/**
 * Run a CLI command vector IN-PROCESS and capture what the `bin` would have
 * printed, so a host can embed Awareness as a library and reuse the exact
 * same JSON contract it built its argv for — no `node cli.js` child process.
 * Never throws: argument/usage errors are mapped to `{ code: 1, stderr }`,
 * mirroring the bin's non-zero exit. `code` is 2 for lock-wait/pre-edit blocks,
 * matching the subprocess exit codes callers already branch on.
 */
export function execCli(argv: string[]): { code: number; stdout: string; stderr: string } {
  let stdout = '';
  try {
    const code = runCli(argv, { write: (chunk) => { stdout += chunk; } });
    return { code, stdout, stderr: '' };
  } catch (error) {
    return { code: 1, stdout, stderr: error instanceof Error ? error.message : String(error) };
  }
}

/** Compare real paths so package-manager bin shims and symlinks are recognized. */
export function isCliEntrypoint(metaUrl = import.meta.url, argv1 = process.argv[1]): boolean {
  if (!argv1) return false;
  try {
    return realpathSync(fileURLToPath(metaUrl)) === realpathSync(argv1);
  } catch {
    return fileURLToPath(metaUrl) === resolve(argv1);
  }
}
