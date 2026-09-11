import { writeCommandDiagnostic, writeCommandPayload } from '../command-output.js';
/** Shared payload normalization for package-owned lifecycle hook adapters. */
import { basename, relative, resolve } from 'node:path';
import { connectDb, resolveDbPath } from '../db-runtime.js';
import { storageScopeForOperation } from '../workspace-policy.js';
import { canonicalizePath } from '../git.js';
import { extractWriteTargetPaths } from '../write-targets.js';
import { resolveHookAgentId } from '../hook-identity.js';
import { normalizeToolHookPayload, toolHookContextEnvelope } from './tool-protocol.js';

export type ShellHookHost = 'claude' | 'codex' | 'cursor' | 'copilot' | 'gemini' | 'opencode';

export type HookRunnerCommand =
  | 'pre-edit'
  | 'post-edit'
  | 'stop-verify'
  | 'notify-deliver'
  | 'session-compact'
  | 'session-end';

export interface HookRunOptions {
  host?: ShellHookHost;
  skillRoot?: string;
}

export interface HookControlOutcome {
  exitCode: number;
  payload?: Record<string, unknown>;
  stderr?: string;
}

export const INTERNAL_HOOK_HOST = '__octocode_hook_host';
export const INTERNAL_SKILL_ROOT = '__octocode_skill_root';

export function parsePayload(raw: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(raw || '{}');
    return parsed && typeof parsed === 'object' ? parsed as Record<string, unknown> : {};
  } catch {
    return raw.trim() ? { input: raw } : {};
  }
}

export function objectOrEmpty(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? value as Record<string, unknown> : {};
}

export function payloadInput(payload: Record<string, unknown>): unknown {
  const input = payload.tool_input ?? payload.toolArgs ?? payload.input ?? payload.args ?? payload;
  if (typeof input !== 'string' || (payload.tool_input === undefined && payload.toolArgs === undefined)) return input;
  try {
    const parsed = JSON.parse(input);
    return parsed && typeof parsed === 'object' ? parsed : input;
  } catch {
    return input;
  }
}

export function payloadForFileExtraction(payload: Record<string, unknown>): unknown {
  const input = payloadInput(payload);
  const inputObj = objectOrEmpty(input);
  if (inputObj === payload) return input;
  if (Object.keys(inputObj).length === 0) return input;
  return { ...payload, ...inputObj };
}

export function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === 'string' && value.trim()) return value.trim();
  }
  return null;
}

export function normalizeShellHookHost(value: unknown): ShellHookHost | null {
  if (typeof value !== 'string') return null;
  const normalized = value.trim().toLowerCase();
  if (normalized === 'github-copilot') return 'copilot';
  if (normalized === 'gemini-cli') return 'gemini';
  return normalized === 'claude'
    || normalized === 'codex'
    || normalized === 'cursor'
    || normalized === 'copilot'
    || normalized === 'gemini'
    || normalized === 'opencode'
    ? normalized
    : null;
}

export function hookCommandForHostEvent(
  host: ShellHookHost,
  eventName: string,
): HookRunnerCommand | null {
  if (host === 'copilot') {
    const commands: Readonly<Record<string, HookRunnerCommand>> = {
      preToolUse: 'pre-edit',
      PreToolUse: 'pre-edit',
      postToolUse: 'post-edit',
      PostToolUse: 'post-edit',
      postToolUseFailure: 'post-edit',
      PostToolUseFailure: 'post-edit',
      agentStop: 'stop-verify',
      Stop: 'stop-verify',
      subagentStop: 'stop-verify',
      SubagentStop: 'stop-verify',
      notification: 'notify-deliver',
      Notification: 'notify-deliver',
      preCompact: 'session-compact',
      PreCompact: 'session-compact',
      sessionEnd: 'session-end',
      SessionEnd: 'session-end',
    };
    return commands[eventName] ?? null;
  }
  if (host === 'gemini') {
    const commands: Readonly<Record<string, HookRunnerCommand>> = {
      BeforeTool: 'pre-edit',
      AfterTool: 'post-edit',
      AfterAgent: 'stop-verify',
      PreCompress: 'session-compact',
      SessionEnd: 'session-end',
    };
    return commands[eventName] ?? null;
  }
  return null;
}

export function shellHookHost(payload: Record<string, unknown>): ShellHookHost {
  const explicit = normalizeShellHookHost(
    payload[INTERNAL_HOOK_HOST]
      ?? process.env.OCTOCODE_AGENT_HOST
      ?? payload.host
      ?? payload.client,
  );
  if (explicit) return explicit;
  const eventName = firstString(payload.hook_event_name, payload.eventName) ?? '';
  if (eventName && eventName[0] === eventName[0]?.toLowerCase()) return 'cursor';
  return 'claude';
}

export function hookSkillRoot(payload: Record<string, unknown>): string | null {
  return firstString(payload[INTERNAL_SKILL_ROOT], process.env.OCTOCODE_SKILL_ROOT);
}

export function hookContextEnvelope(
  host: ShellHookHost,
  eventName: string,
  message: string,
): Record<string, unknown> {
  if (eventName.toLowerCase().includes('tool')) {
    return toolHookContextEnvelope(host, eventName, message) ?? {};
  }
  if (host === 'cursor') {
    if (eventName === 'sessionStart') return { additional_context: message };
    return { permission: 'allow', agent_message: message };
  }
  if (host === 'copilot') {
    if (eventName === 'preToolUse' || eventName === 'PreToolUse') {
      return { permissionDecision: 'allow', additionalContext: message };
    }
    return { additionalContext: message };
  }
  return {
    hookSpecificOutput: {
      hookEventName: eventName,
      additionalContext: message,
    },
  };
}

export function hookBlockOutcome(
  host: ShellHookHost,
  phase: 'pre-edit' | 'stop',
  message: string,
): HookControlOutcome {
  if (host === 'copilot') {
    if (phase === 'stop') {
      return { exitCode: 0, payload: { decision: 'block', reason: message } };
    }
    return {
      exitCode: 0,
      payload: {
        permissionDecision: 'deny',
        permissionDecisionReason: message,
      },
    };
  }
  if (host !== 'cursor') return { exitCode: 2, stderr: message };
  if (phase === 'stop') {
    return { exitCode: 0, payload: { followup_message: message } };
  }
  return {
    exitCode: 0,
    payload: {
      permission: 'deny',
      user_message: message,
      agent_message: message,
    },
  };
}

export function writeHookPayload(payload: Record<string, unknown>): void {
  writeCommandPayload(payload, true);
}

export function emitHookContext(payload: Record<string, unknown>, eventName: string, message: string): void {
  writeHookPayload(hookContextEnvelope(shellHookHost(payload), eventName, message));
}

export function completeHookControl(outcome: HookControlOutcome): number {
  if (outcome.payload) writeHookPayload(outcome.payload);
  if (outcome.stderr) writeCommandDiagnostic(outcome.stderr);
  return outcome.exitCode;
}

export function agentId(payload: Record<string, unknown>): string {
  return resolveHookAgentId(payload, objectOrEmpty(payloadInput(payload)));
}

export function sessionId(payload: Record<string, unknown>): string | null {
  const input = objectOrEmpty(payloadInput(payload));
  return firstString(
    payload.session_id, payload.sessionId, input.session_id, input.sessionId,
  );
}

export function promptQuery(payload: Record<string, unknown>): string | null {
  const input = objectOrEmpty(payloadInput(payload));
  const prompt = firstString(
    payload.prompt,
    payload.user_prompt,
    payload.userPrompt,
    payload.text,
    payload.message,
    typeof payload.input === 'string' ? payload.input : null,
    input.prompt,
    input.user_prompt,
    input.userPrompt,
    input.text,
    input.message,
  );
  return prompt ? prompt.slice(0, 4_000) : null;
}

export function hookSessionCorrelation(payload: Record<string, unknown>): string | null {
  const input = objectOrEmpty(payloadInput(payload));
  return firstString(
    sessionId(payload),
    payload.transcript_path,
    payload.transcriptPath,
    payload.conversation_id,
    payload.conversationId,
    payload.thread_id,
    payload.threadId,
    input.transcript_path,
    input.transcriptPath,
    input.conversation_id,
    input.conversationId,
    input.thread_id,
    input.threadId,
  );
}

export function toolName(payload: Record<string, unknown>): string {
  const input = objectOrEmpty(payloadInput(payload));
  return firstString(
    payload.tool_name, payload.toolName, payload.name, input.tool_name, input.toolName,
  ) ?? '';
}

// Build an informative auto-claim rationale from the tool + target files so a
// blocked agent sees WHAT the holder is doing, not a generic "file edit".
export function autoClaimRationale(payload: Record<string, unknown>, files: string[]): string {
  const tool = toolName(payload);
  const names = files.map((f) => f.split('/').pop() || f);
  const shown = names.slice(0, 3).join(', ');
  const extra = names.length > 3 ? ` +${names.length - 3} more` : '';
  const action = tool ? `${tool}` : 'edit';
  return `auto: ${action} ${shown}${extra} (lifecycle hook)`;
}

export function fallbackVerificationPlan(files: string[], cwd: string): string {
  const canonicalWorkspace = canonicalizePath(cwd);
  const normalized = [...new Set(files.map(file => resolveHookPath(file, cwd)))];
  const shown = normalized.slice(0, 3)
    .map(file => relative(canonicalWorkspace, file) || basename(file))
    .join(', ');
  const omitted = normalized.length > 3 ? ` (+${normalized.length - 3} more)` : '';
  return `Verify ${shown || 'the edited files'}${omitted}: run the smallest relevant test/typecheck and inspect the diff; record the check and result.`;
}

export function agentName(payload: Record<string, unknown>): string {
  return firstString(
    payload.agent_name, payload.agentName,
    payload.agent_display_name, payload.agentDisplayName, process.env.OCTOCODE_AGENT_NAME,
  ) ?? '';
}

/** Registry labels require explicit facts; shellHookHost's protocol fallback is not identity evidence. */
export function agentHost(payload: Record<string, unknown>): string | null {
  return firstString(
    payload[INTERNAL_HOOK_HOST], payload.agent_host, payload.agentHost,
    payload.host, payload.client, process.env.OCTOCODE_AGENT_HOST,
  );
}

export function agentVendor(payload: Record<string, unknown>): string | null {
  // A child hook's observed provider takes precedence over inherited parent environment.
  return firstString(
    payload.agent_vendor, payload.agentVendor,
    objectOrEmpty(payload.model).provider, payload.provider, process.env.OCTOCODE_AGENT_VENDOR,
  );
}

export function workspace(payload: Record<string, unknown>): string | null {
  const value = payload.cwd ?? payload.workspace ?? payload.workspacePath;
  return typeof value === 'string' && value.trim() ? value.trim() : null;
}

export function artifact(payload: Record<string, unknown>): string | null {
  const input = objectOrEmpty(payloadInput(payload));
  const value =
    process.env.OCTOCODE_ARTIFACT
    ?? process.env.OCTOCODE_PACKAGE
    ?? process.env.OCTOCODE_SERVICE
    ?? payload.artifact
    ?? payload.package
    ?? payload.service
    ?? input.artifact
    ?? input.package
    ?? input.service;
  return typeof value === 'string' && value.trim() ? value.trim() : null;
}

export function hookReason(payload: Record<string, unknown>): string {
  return typeof payload.reason === 'string' ? payload.reason : '';
}

export function isStopHookActive(payload: Record<string, unknown>): boolean {
  return Boolean(payload.stop_hook_active);
}

export function hookEventName(payload: Record<string, unknown>): string | null {
  return firstString(payload.hook_event_name, payload.eventName);
}

export function hookToolFailed(payload: Record<string, unknown>): boolean {
  const host = shellHookHost(payload);
  const toolEvent = hookEventName(payload);
  if (toolEvent) {
    try {
      const outcome = normalizeToolHookPayload(payload, host).outcome;
      return outcome.terminal && outcome.kind !== 'success';
    } catch { /* Non-tool lifecycle events use the generic failure fields below. */ }
  }
  const input = objectOrEmpty(payloadInput(payload));
  const response = objectOrEmpty(payload.tool_response ?? payload.toolResponse ?? payload.result);
  const event = hookEventName(payload)?.toLowerCase() ?? '';
  return event.includes('failure')
    || payload.is_error === true
    || payload.isError === true
    || input.is_error === true
    || input.isError === true
    || response.is_error === true
    || response.isError === true
    || response.success === false;
}

export function extractFiles(payload: Record<string, unknown>): string[] {
  const input = payloadForFileExtraction(payload);
  const inputObj = objectOrEmpty(input);
  const toolName = payload.tool_name ?? payload.toolName ?? payload.name ?? inputObj.tool_name ?? inputObj.toolName ?? '';
  return extractWriteTargetPaths(toolName, input);
}

export function resolveHookPath(file: string, cwd = process.cwd()): string {
  // Absolutize AND normalize: `..`/`.` segments and non-absolute inputs (Codex
  // apply_patch and Cursor payloads often carry repo-relative paths) must be
  // collapsed before any containment check, or a traversal path that actually
  // resolves inside the skill root can slip past a textual prefix comparison.
  return canonicalizePath(resolve(cwd, file));
}

export function db(payload: Record<string, unknown>, operation = 'host.hook') {
  const cwd = workspace(payload) ?? process.cwd();
  return connectDb(resolveDbPath(null, {
    scope: storageScopeForOperation(operation, cwd),
    workspace: cwd,
  }));
}
