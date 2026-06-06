import type { PersistedSession } from './types.js';
import { writeSessionToDisk, readSessionFromDisk } from './sessionDiskIO.js';

const FLUSH_INTERVAL_MS = 60_000;

let cachedSession: PersistedSession | null = null;

let isDirty = false;

let flushTimer: ReturnType<typeof setInterval> | null = null;

let exitHandlersRegistered = false;

let exitListener: (() => void) | null = null;
let sigintListener: (() => void) | null = null;
let sigtermListener: (() => void) | null = null;

let isFlushing = false;

function registerExitHandlers(): void {
  if (exitHandlersRegistered) return;
  exitHandlersRegistered = true;

  exitListener = () => {
    flushSessionSync();
  };
  sigintListener = () => {
    flushSessionSync();
  };
  sigtermListener = () => {
    flushSessionSync();
  };

  process.on('exit', exitListener);
  process.on('SIGINT', sigintListener);
  process.on('SIGTERM', sigtermListener);
}

export function unregisterExitHandlers(): void {
  if (exitListener) {
    process.removeListener('exit', exitListener);
    exitListener = null;
  }
  if (sigintListener) {
    process.removeListener('SIGINT', sigintListener);
    sigintListener = null;
  }
  if (sigtermListener) {
    process.removeListener('SIGTERM', sigtermListener);
    sigtermListener = null;
  }
  exitHandlersRegistered = false;
}

function startFlushTimer(): void {
  if (flushTimer) return;

  flushTimer = setInterval(() => {
    if (isDirty && cachedSession) {
      writeSessionToDisk(cachedSession);
      isDirty = false;
    }
  }, FLUSH_INTERVAL_MS);

  flushTimer.unref();
}

export function stopFlushTimer(): void {
  if (flushTimer) {
    clearInterval(flushTimer);
    flushTimer = null;
  }
}

export function readSession(): PersistedSession | null {
  if (cachedSession) {
    return cachedSession;
  }

  const session = readSessionFromDisk();
  if (session) {
    cachedSession = session;
  }
  return session;
}

export function writeSession(session: PersistedSession): void {
  cachedSession = session;
  isDirty = true;

  registerExitHandlers();
  startFlushTimer();
}

export function flushSession(): void {
  if (isDirty && cachedSession) {
    writeSessionToDisk(cachedSession);
    isDirty = false;
  }
}

export function flushSessionSync(): void {
  if (isFlushing) return;
  if (isDirty && cachedSession) {
    isFlushing = true;
    try {
      writeSessionToDisk(cachedSession);
      isDirty = false;
    } catch {
      void 0;
    } finally {
      isFlushing = false;
    }
  }
}

export function clearCache(): void {
  cachedSession = null;
  isDirty = false;
  isFlushing = false;
}

export function resetCacheState(): void {
  clearCache();
  stopFlushTimer();
  unregisterExitHandlers();
}
