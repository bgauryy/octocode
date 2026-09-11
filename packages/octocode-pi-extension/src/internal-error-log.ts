import fs from 'node:fs';
import path from 'node:path';
import {
  ensurePrivateDirectory,
  hardenPrivateFile,
  PRIVATE_FILE_MODE,
} from '@octocodeai/agent-contracts/permissions';
import { extensionWorkspaceRoot } from './extension-paths.js';
import { createSessionArtifactContext } from './tools/session-artifacts.js';
import type { PiContext } from './types.js';

export function getInternalErrorLogPath(
  cwd = process.cwd(),
  sessionManager?: { getSessionId?(): string | undefined; getSessionFile?(): string | undefined },
): string {
  if (sessionManager) {
    try {
      return createSessionArtifactContext({ cwd, sessionManager }).resolve('logs/error.txt');
    } catch { /* fallback when cwd is unavailable */ }
  }
  return path.join(extensionWorkspaceRoot(cwd), 'logs', 'error.txt');
}

function normalizeError(error: unknown): { name?: string; message: string; stack?: string; cause?: string } {
  if (error instanceof Error) {
    return {
      name: error.name,
      message: error.message,
      stack: error.stack,
      cause: error.cause === undefined ? undefined : String(error.cause),
    };
  }
  return { message: String(error) };
}

function redactForLog(value: unknown, depth = 0, seen = new WeakSet<object>()): unknown {
  if (value === null || value === undefined) return value;
  if (typeof value === 'string') {
    return value
      .replace(/Bearer\s+[A-Za-z0-9._~+/-]+=*/gi, 'Bearer [REDACTED]')
      .replace(/(api[_-]?key|token|secret|password)=([^\s&]+)/gi, '$1=[REDACTED]');
  }
  if (typeof value !== 'object') return value;
  if (seen.has(value)) return '[Circular]';
  if (depth >= 6) return '[MaxDepth]';
  seen.add(value);
  if (Array.isArray(value)) return value.slice(0, 50).map((item) => redactForLog(item, depth + 1, seen));
  const out: Record<string, unknown> = {};
  for (const [key, item] of Object.entries(value as Record<string, unknown>).slice(0, 100)) {
    out[key] = /authorization|cookie|set-cookie|token|secret|password|api[_-]?key|access[_-]?key|credential/i.test(key)
      ? '[REDACTED]'
      : redactForLog(item, depth + 1, seen);
  }
  return out;
}

function safeJson(value: unknown): string {
  try { return JSON.stringify(redactForLog(value), null, 2); }
  catch { return String(value); }
}

function formatContextForLog(ctx: PiContext | undefined): string[] {
  const usage = ctx?.getContextUsage?.();
  return [
    `cwd: ${ctx?.cwd ?? process.cwd()}`,
    ctx?.mode ? `mode: ${ctx.mode}` : '',
    ctx?.model?.id ? `model: ${ctx.model.id}` : '',
    ctx?.model ? `modelReasoning: ${String(ctx.model.reasoning)}` : '',
    usage && usage.tokens != null ? `context: ${usage.tokens}/${usage.contextWindow} (${Math.round((usage.tokens / usage.contextWindow) * 100)}%)` : usage ? 'context: unknown (post-compaction)' : '',
  ].filter(Boolean);
}

export interface InternalErrorLogOptions {
  severity?: 'error' | 'warning';
  stack?: boolean;
}

export function logInternalError(
  source: string,
  error: unknown,
  details: Record<string, unknown> = {},
  ctx?: PiContext,
  options: InternalErrorLogOptions = {},
): void {
  try {
    const severity = options.severity ?? 'error';
    const includeStack = options.stack ?? severity === 'error';
    const logPath = getInternalErrorLogPath(ctx?.cwd ?? process.cwd(), ctx?.sessionManager);
    const normalized = normalizeError(error);
    const durationMs = typeof details['durationMs'] === 'number' ? details['durationMs'] : undefined;
    const redactedDetails = Object.keys(details).length > 0 ? safeJson(details) : '';
    ensurePrivateDirectory(path.dirname(logPath));
    hardenPrivateFile(logPath);
    fs.appendFileSync(logPath, [
      severity === 'warning' ? '=== Octocode Pi Extension Warning ===' : '=== Octocode Pi Extension Error ===',
      `timestamp: ${new Date().toISOString()}`,
      `uptimeMs: ${Math.round(process.uptime() * 1000)}`,
      `source: ${source}`,
      `severity: ${severity}`,
      durationMs === undefined ? '' : `durationMs: ${durationMs}`,
      ...formatContextForLog(ctx),
      normalized.name ? `error.name: ${normalized.name}` : '',
      `error.message: ${normalized.message}`,
      normalized.cause ? `error.cause: ${normalized.cause}` : '',
      redactedDetails ? `details: ${redactedDetails}` : '',
      includeStack && normalized.stack ? `stack:\n${normalized.stack}` : '',
      '---',
    ].filter(Boolean).join('\n') + '\n', { encoding: 'utf8', mode: PRIVATE_FILE_MODE });
    hardenPrivateFile(logPath);
  } catch {
    // Logging must never become the reason the extension fails.
  }
}
