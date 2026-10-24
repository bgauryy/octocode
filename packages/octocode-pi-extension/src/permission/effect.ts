import { execFile } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { formatSize } from '@earendil-works/pi-coding-agent';
import { redactSecrets, sanitizeTerminalText } from '../shared/sanitize.js';
import type { Snapshot } from './snapshot.js';
import { plural } from '../shared/render.js';
import { clipText, contentText } from '../shared/util.js';
import type { PermissionRequest } from './request.js';
import { actionWhy, decide, type ActionClass, type PermissionMode } from './mode.js';
import { catastrophicCommand, expandHome, removalTargets } from './shell.js';

/**
 * The effect card every prompt leads with: what the call does, how to undo it, why it asks and what is risky about
 * it. The lines are computed from the call and the system, never taken from the agent; what the agent says about its
 * own purpose comes last and is labelled unverified.
 */

/** What the card says when a line cannot be computed in time. */
export const UNKNOWN = 'unknown';
/** How long the card may take to compute; a line that is not ready says `unknown`. */
const CARD_BUDGET_MS = 50;
/** What a call with no stated purpose says. */
export const NO_REASON = 'no reason given';
/** The longest agent text one card or audit line keeps. */
const AGENT_TEXT_MAX = 160;

interface EffectCard {
  effect: string;
  undo: string;
  risk: string;
}

/** One line of agent-chosen text: terminal escapes and bidi controls removed, line breaks shown as `⏎`. */
export const foldLines = (text: string): string => sanitizeTerminalText(text.replace(/\r\n|\r|\n/g, '⏎'));

/** An audit line: one line, secrets redacted, at most 160 characters. */
export const auditLine = (text: string): string => clipText(redactSecrets(foldLines(text)), AGENT_TEXT_MAX);

/** The targets an audit entry names: five, one line each, then how many more. */
export function auditLines(lines: readonly string[]): string[] {
  const shown = lines.slice(0, 5).map(auditLine);
  return lines.length > 5 ? [...shown, `+${lines.length - 5} more`] : shown;
}

const first = (request: PermissionRequest): string => request.what[0] ?? '';

/** Each class's lines when the call states none: what it does, whether it can be taken back, what to watch. */
const DEFAULTS: Record<ActionClass, (request: PermissionRequest) => EffectCard> = {
  'read.outside': (request) => ({ effect: `reads ${first(request)}`, undo: 'nothing to undo (read only)', risk: 'what it reads enters the model context' }),
  'file.workspace': (request) => ({ effect: `changes ${first(request)}`, undo: 'none (not checkpointed)', risk: 'none found' }),
  'file.outside': (request) => ({ effect: `changes ${first(request)}`, undo: 'none (not checkpointed)', risk: 'a file outside the repository, or one that runs in later sessions' }),
  shell: (request) => ({ effect: `runs ${first(request)}`, undo: 'none: a shell command is not checkpointed', risk: UNKNOWN }),
  'shell.destructive': (request) => ({ effect: `runs ${first(request)}`, undo: 'none', risk: UNKNOWN }),
  web: (request) => ({ effect: `requests ${first(request)}`, undo: 'nothing to undo', risk: 'the page text is untrusted' }),
  browser: (request) => ({ effect: `browser ${first(request)}`, undo: 'none', risk: 'a page action may submit or change data' }),
  'browser.control': (request) => ({ effect: `controls the browser: ${first(request)}`, undo: 'none', risk: 'a real browser with its logins, cookies and open tabs' }),
  'net.private': (request) => ({ effect: `opens ${first(request)}`, undo: 'none', risk: 'reaches internal services that public-address checks keep out' }),
  'upload.outside': (request) => ({ effect: `sends ${request.what.join(', ')} to the page`, undo: 'none: a sent file cannot be taken back', risk: 'file contents leave this machine' }),
  'mcp.write': (request) => ({ effect: `calls ${first(request)}`, undo: 'none', risk: UNKNOWN }),
  'mcp.destructive': (request) => ({ effect: `calls ${first(request)}`, undo: 'none', risk: 'the tool declares itself destructive' }),
  'agent.spawn': (request) => ({ effect: `starts ${first(request)}`, undo: 'stop it with /agents stop', risk: 'it acts on its own with the tools of its profile' }),
};

/** The card for a call with no class (a reason forced the ask). */
const CALL: (request: PermissionRequest) => EffectCard = (request) => ({ effect: `runs ${first(request)}`, undo: 'none', risk: UNKNOWN });

/** `value` after at most `ms`, else `unknown`. */
function within(value: string | Promise<string>, ms: number): Promise<string> {
  if (typeof value === 'string') return Promise.resolve(value);
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(UNKNOWN), Math.max(0, ms));
    value.then(
      (text) => {
        clearTimeout(timer);
        resolve(text);
      },
      () => {
        clearTimeout(timer);
        resolve(UNKNOWN);
      },
    );
  });
}

function expand(target: string, cwd: string): string {
  return path.resolve(cwd, expandHome(target, os.homedir()));
}

/** Files, folders and bytes under `targets`; undefined when the deadline passes first. */
function measureRemoval(targets: readonly string[], cwd: string, deadline: number): { files: number; folders: number; bytes: number } | undefined {
  const total = { files: 0, folders: 0, bytes: 0 };
  const pending = targets.map((target) => expand(target, cwd));
  for (let item = pending.pop(); item !== undefined; item = pending.pop()) {
    if (Date.now() > deadline) return undefined;
    let stat: fs.Stats;
    try {
      stat = fs.lstatSync(item);
    } catch {
      continue;
    }
    if (stat.isDirectory()) {
      total.folders += 1;
      try {
        for (const entry of fs.readdirSync(item)) pending.push(path.join(item, entry));
      } catch {
        // Unreadable: counted as a folder only.
      }
    } else {
      total.files += 1;
      total.bytes += stat.size;
    }
  }
  return total;
}

/** How many files under `targets` git does not track (nor a snapshot hold); undefined when git cannot say in time. */
function untracked(targets: readonly string[], cwd: string, timeout: number): Promise<number | undefined> {
  return new Promise((resolve) => {
    execFile('git', ['ls-files', '--others', '-z', '--', ...targets.map((target) => expand(target, cwd))], { cwd, timeout: Math.max(1, timeout), encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' } }, (error, stdout) => {
      resolve(error ? undefined : stdout.split('\0').filter(Boolean).length);
    });
  });
}

/** The effect and risk of a destructive shell command: what it removes, and what a snapshot does not hold. */
async function destructive(request: PermissionRequest, snapshot: Snapshot | undefined, deadline: number): Promise<{ effect: string | Promise<string>; risk: string | Promise<string> }> {
  const command = first(request);
  const cwd = request.cwd ?? process.cwd();
  const danger = catastrophicCommand(command, cwd) ?? 'a destructive command';
  const targets = removalTargets(command);
  const measured = targets.length > 0 ? measureRemoval(targets, cwd, deadline) : undefined;
  const effect = measured
    ? `${danger}: removes ${plural(measured.files, 'file')} and ${plural(measured.folders, 'folder')} (${formatSize(measured.bytes)}) under ${targets.join(', ')}`
    : `${danger}; what it removes: ${UNKNOWN}`;
  if (!snapshot) return { effect, risk: UNKNOWN };
  if ('none' in snapshot) return { effect, risk: `nothing can be restored: no snapshot (${snapshot.none})` };
  if (targets.length === 0) return { effect, risk: 'untracked files are not in the snapshot' };
  const risk = untracked(targets, cwd, deadline - Date.now()).then((count) => (count === undefined ? UNKNOWN : count === 0 ? 'none found: tracked files are in the snapshot' : `${plural(count, 'untracked or ignored file')} not in the snapshot`));
  return { effect, risk };
}

/** The card's undo line for a call with a snapshot: where it is, and how to get a path back. */
const undoOf = (snapshot: Snapshot): string => ('ref' in snapshot ? `snapshot ${snapshot.ref}; git checkout ${snapshot.ref} -- <path> restores tracked files` : `none (${snapshot.none})`);

/**
 * The card for `request`: lines the caller set stand, the rest are computed per class within `CARD_BUDGET_MS`; a line
 * that is not ready says `unknown`. `snapshot` is what was taken before a destructive shell call.
 */
export async function computeCard(request: PermissionRequest, snapshot?: Snapshot): Promise<EffectCard> {
  const deadline = Date.now() + CARD_BUDGET_MS;
  const base = request.action ? DEFAULTS[request.action](request) : CALL(request);
  const lines: { effect: string | Promise<string>; undo: string; risk: string | Promise<string> } = { ...base };
  if (request.action === 'shell.destructive') {
    const found = await destructive(request, snapshot, deadline);
    lines.effect = found.effect;
    lines.risk = found.risk;
    lines.undo = snapshot ? undoOf(snapshot) : 'none';
  }
  const left = () => deadline - Date.now();
  const [effect, risk] = await Promise.all([within(request.effect ?? lines.effect, left()), within(request.risk ?? lines.risk, left())]);
  return { effect, undo: request.undo ?? lines.undo, risk };
}

/**
 * The reason lines of a request: why this class asks in `mode` (when it does), then each system reason the caller
 * named. These come from the code, never from the agent.
 */
function reasonLines(request: PermissionRequest, mode: PermissionMode): string[] {
  const lines: string[] = [];
  for (const action of request.action ? [request.action, ...(request.also ?? [])] : []) {
    if (decide(mode, action) === 'ask') lines.push(`${actionWhy(action)} (${mode} mode asks)`);
  }
  if (request.reason) lines.push(...request.reason.split('\n').filter(Boolean).map((line) => clipText(foldLines(line), 400)));
  return lines;
}

/** The system's lines of the card: effect, undo, why it asks and risk, each led by `▸`, which agent text cannot forge: it is one line (breaks shown as `⏎`) and comes after them. */
export function systemLines(request: PermissionRequest, mode: PermissionMode, indent = '  '): string[] {
  return [
    ...(request.effect !== undefined ? [`${indent}▸ effect: ${foldLines(request.effect)}`] : []),
    ...(request.undo !== undefined ? [`${indent}▸ undo: ${foldLines(request.undo)}`] : []),
    ...reasonLines(request, mode).map((line) => `${indent}▸ why it asks: ${line}`),
    ...(request.risk !== undefined ? [`${indent}▸ risk: ${foldLines(request.risk)}`] : []),
    ...(request.nested ? [`${indent}▸ issued by a codemode script`] : []),
  ];
}

/** The card as dialog lines: the system's lines, then what the agent says about its purpose, unverified and last. */
export function cardLines(request: PermissionRequest, mode: PermissionMode, indent = '  '): string[] {
  return [
    ...systemLines(request, mode, indent),
    ...(request.purpose === '' ? [] : [`${indent}agent says (unverified): ${clipText(foldLines(request.purpose ?? NO_REASON), AGENT_TEXT_MAX)}`]),
  ];
}

/** The last sentence of `text`, one line, at most 160 characters; undefined when it has none. */
export function lastSentence(text: string): string | undefined {
  const sentences = text.split(/(?<=[.!?:])\s+|\n+/).map((sentence) => sentence.trim()).filter(Boolean);
  const last = sentences.at(-1);
  return last ? clipText(foldLines(last), AGENT_TEXT_MAX) : undefined;
}

interface BranchEntry {
  type?: string;
  message?: { role?: string; content?: unknown };
}

/**
 * What the assistant said in the message that issued tool call `toolCallId`: its last sentence, read from the session's
 * current branch. Undefined when the call or its text is not there.
 */
export function transcriptPurpose(sessionManager: { getBranch?: () => unknown[] } | undefined, toolCallId: string | undefined): string | undefined {
  if (!toolCallId || !sessionManager?.getBranch) return undefined;
  try {
    const branch = sessionManager.getBranch() as BranchEntry[];
    for (let at = branch.length - 1; at >= 0; at--) {
      const message = branch[at]!.type === 'message' ? branch[at]!.message : undefined;
      if (message?.role !== 'assistant' || !Array.isArray(message.content)) continue;
      const parts = message.content as Array<{ type?: string; id?: string; text?: string }>;
      if (!parts.some((part) => part.type === 'toolCall' && part.id === toolCallId)) continue;
      return lastSentence(contentText(parts));
    }
  } catch {
    // A session manager that cannot read its branch has no transcript to quote.
  }
  return undefined;
}
