export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface LogEntry {
  level: LogLevel;
  module: string;
  message: string;
  data?: Record<string, unknown>;
}

type LogHandler = (entry: LogEntry) => void;

let customHandler: LogHandler | null = null;

function defaultHandler(entry: LogEntry): void {
  const prefix = `[${entry.module}]`;
  const dataStr =
    entry.data && Object.keys(entry.data).length > 0
      ? ` ${JSON.stringify(entry.data)}`
      : '';
  process.stderr.write(
    `${prefix} ${entry.level}: ${entry.message}${dataStr}\n`
  );
}

export function setLogHandler(handler: LogHandler | null): void {
  customHandler = handler;
}

export function _getLogHandler(): LogHandler | null {
  return customHandler;
}

export function createLogger(module: string) {
  function log(
    level: LogLevel,
    message: string,
    data?: Record<string, unknown>
  ) {
    const entry: LogEntry = { level, module, message, ...(data && { data }) };
    (customHandler ?? defaultHandler)(entry);
  }

  return {
    debug: (message: string, data?: Record<string, unknown>) =>
      log('debug', message, data),
    info: (message: string, data?: Record<string, unknown>) =>
      log('info', message, data),
    warn: (message: string, data?: Record<string, unknown>) =>
      log('warn', message, data),
    error: (message: string, data?: Record<string, unknown>) =>
      log('error', message, data),
  };
}
