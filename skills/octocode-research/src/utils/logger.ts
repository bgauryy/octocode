import { errorLog, warnLog } from './colors.js';
import { errorQueue } from './errorQueue.js';


import fs from 'node:fs';
import { promises as fsAsync } from 'node:fs';
import path from 'node:path';
import os from 'node:os';


const HOME = os.homedir();
const OCTOCODE_DIR = process.env.OCTOCODE_HOME || path.join(HOME, '.octocode');
const LOGS_DIR = path.join(OCTOCODE_DIR, 'logs');
const ERROR_LOG = path.join(LOGS_DIR, 'errors.log');
const TOOLS_LOG = path.join(LOGS_DIR, 'tools.log');

const MAX_LOG_SIZE = 10 * 1024 * 1024;

const MAX_LOG_DATA_SIZE = 100 * 1024;


let initialized = false;
let fileLoggingEnabled = true;
let initPromise: Promise<void> | null = null;


async function ensureLogsDirAsync(): Promise<void> {
  if (initialized) return;
  if (initPromise) return initPromise;

  initPromise = (async () => {
    try {
      await fsAsync.mkdir(OCTOCODE_DIR, { recursive: true, mode: 0o700 });
      await fsAsync.mkdir(LOGS_DIR, { recursive: true, mode: 0o700 });
      initialized = true;
    } catch (err) {
      process.stderr.write(
        `[Logger] Failed to create logs directory: ${err}\n` +
        `Falling back to console-only logging.\n`
      );
      fileLoggingEnabled = false;
    }
  })();

  return initPromise;
}


function ensureLogsDirSync(): void {
  if (initialized) return;

  try {
    if (!fs.existsSync(OCTOCODE_DIR)) {
      fs.mkdirSync(OCTOCODE_DIR, { recursive: true, mode: 0o700 });
    }
    if (!fs.existsSync(LOGS_DIR)) {
      fs.mkdirSync(LOGS_DIR, { recursive: true, mode: 0o700 });
    }
    initialized = true;
  } catch (err) {
    process.stderr.write(
      `[Logger] Failed to create logs directory: ${err}\n` +
      `Falling back to console-only logging.\n`
    );
    fileLoggingEnabled = false;
  }
}


async function rotateIfNeededAsync(logPath: string): Promise<void> {
  try {
    const stats = await fsAsync.stat(logPath).catch(() => null);
    if (!stats || stats.size < MAX_LOG_SIZE) return;

    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const ext = path.extname(logPath);
    const base = path.basename(logPath, ext);
    const rotatedPath = path.join(LOGS_DIR, `${base}.${timestamp}${ext}`);
    
    await fsAsync.rename(logPath, rotatedPath);
    await cleanupOldLogsAsync(base, ext, 5);
  } catch {
    void 0;
  }
}


async function cleanupOldLogsAsync(baseName: string, ext: string, keep: number): Promise<void> {
  try {
    const files = await fsAsync.readdir(LOGS_DIR);
    const rotatedFiles = files
      .filter((f) => f.startsWith(baseName + '.') && f.endsWith(ext) && f !== baseName + ext)
      .sort()
      .reverse();

    const toDelete = rotatedFiles.slice(keep);
    await Promise.all(
      toDelete.map((f) => fsAsync.unlink(path.join(LOGS_DIR, f)).catch(err => errorQueue.push(err, 'cleanupOldLogs')))
    );
  } catch {
    void 0;
  }
}


function safeStringify(data: unknown): string {
  if (data === undefined) return '';

  try {
    let size = 0;
    let truncated = false;
    const seen = new WeakSet<object>();

    const replacer = (_key: string, value: unknown): unknown => {
      if (value !== null && typeof value === 'object') {
        if (seen.has(value)) {
          return '[Circular]';
        }
        seen.add(value);
      }

      const valueStr = typeof value === 'string' ? value : String(value ?? '');
      size += valueStr.length + 10;

      if (size > MAX_LOG_DATA_SIZE && !truncated) {
        truncated = true;
      }

      return value;
    };

    const result = JSON.stringify(data, replacer, 2);

    if (truncated) {
      return JSON.stringify({
        _truncated: true,
        _estimatedSize: size,
        _message: 'Data too large for logging',
      }, null, 2);
    }

    return result;
  } catch {
    return '[Circular or non-serializable data]';
  }
}


function formatLogEntry(level: string, message: string, data?: unknown): string {
  const timestamp = new Date().toISOString();
  const dataStr = data !== undefined ? `\n${safeStringify(data)}` : '';
  return `[${timestamp}] [${level}] ${message}${dataStr}\n`;
}


function writeLogAsync(logPath: string, entry: string): void {
  if (!fileLoggingEnabled) return;

  (async () => {
    try {
      await ensureLogsDirAsync();
      await rotateIfNeededAsync(logPath);
      await fsAsync.appendFile(logPath, entry, { encoding: 'utf-8' });
    } catch (err) {
      fileLoggingEnabled = false;
      process.stderr.write(`[Logger] File write failed, disabling: ${err}\n`);
    }
  })();
}


export function logError(message: string, error?: Error | unknown): void {
  const errorData =
    error instanceof Error
      ? { name: error.name, message: error.message, stack: error.stack }
      : error;

  const entry = formatLogEntry('ERROR', message, errorData);
  writeLogAsync(ERROR_LOG, entry);

  const consoleError = process.env.NODE_ENV === 'development'
    ? error
    : (error instanceof Error ? error.message : String(error || ''));
  console.error('%s %o', errorLog('[ERROR] ' + message), consoleError);
}


export function logWarn(message: string, data?: unknown): void {
  const entry = formatLogEntry('WARN', message, data);
  writeLogAsync(ERROR_LOG, entry);
  console.warn('%s', warnLog('[WARN] ' + message));
}


interface ToolLogEntry {
  tool: string;
  route: string;
  method: string;
  params: Record<string, unknown>;
  duration?: number;
  success: boolean;
  error?: string;
  resultSize?: number;
  requestId?: string;
}

export function logToolCall(entry: ToolLogEntry): void {
  const logEntry = formatLogEntry('TOOL', `${entry.method} ${entry.route}`, entry);
  writeLogAsync(TOOLS_LOG, logEntry);
}


export function getLogsPath(): string {
  return LOGS_DIR;
}


export function initializeLogger(): void {
  ensureLogsDirSync();
}


const SENSITIVE_KEYS = ['token', 'key', 'secret', 'password', 'auth', 'credential', 'api_key', 'apikey'];


export function sanitizeQueryParams(query: Record<string, unknown>): Record<string, unknown> {
  const sanitized: Record<string, unknown> = {};

  for (const [key, value] of Object.entries(query)) {
    const isSensitive = SENSITIVE_KEYS.some((s) => key.toLowerCase().includes(s));
    sanitized[key] = isSensitive ? '[REDACTED]' : value;
  }

  return sanitized;
}
