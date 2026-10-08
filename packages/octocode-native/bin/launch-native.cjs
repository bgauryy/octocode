'use strict';

const { spawn } = require('child_process');
const { constants: osConstants } = require('os');
const { resolveNativeBinaryPath } = require('./resolve-binary.cjs');

const FORWARDED_SIGNALS = ['SIGINT', 'SIGTERM', 'SIGHUP'];

/**
 * Make `handler` the only listener for SIGINT/SIGTERM/SIGHUP until the
 * returned `restore()` runs; `restore()` puts the previous listeners back and
 * is safe to call more than once.
 *
 * @param {(signal: NodeJS.Signals) => void} handler
 * @returns {() => void} restore
 */
function takeSignals(handler) {
  const saved = FORWARDED_SIGNALS.map(signal => [signal, process.listeners(signal)]);
  for (const signal of FORWARDED_SIGNALS) {
    process.removeAllListeners(signal);
    process.on(signal, handler);
  }
  let restored = false;
  return () => {
    if (restored) return;
    restored = true;
    for (const [signal, listeners] of saved) {
      process.removeListener(signal, handler);
      for (const listener of listeners) process.on(signal, listener);
    }
  };
}

/**
 * Run `command args` with inherited stdio and resolve its exit status: the
 * child's code, or 128+N on a signal death so OOM (SIGKILL→137) and crashes
 * (SIGSEGV→139) stay distinguishable from an ordinary error exit. Rejects when
 * the process cannot start.
 *
 * Async spawn (not spawnSync) so a signal sent only to this process (e.g.
 * SIGTERM from a supervisor, which does not hit the whole process group) is
 * forwarded to the child instead of queuing behind a blocking wait. While the
 * child runs, forwarding is the only handler for those signals, so a parent
 * handler cannot race the child's own interrupt handling.
 *
 * @param {string} command
 * @param {readonly string[]} args
 * @param {{ env?: NodeJS.ProcessEnv }} [options]
 * @returns {Promise<number>}
 */
function runForwardingSignals(command, args, options = {}) {
  const child = spawn(command, [...args], { stdio: 'inherit', env: options.env ?? process.env });
  const restore = takeSignals(signal => {
    if (child.exitCode === null && child.signalCode === null) child.kill(signal);
  });
  return new Promise((resolve, reject) => {
    child.once('error', error => {
      restore();
      reject(error);
    });
    child.once('close', (code, signal) => {
      restore();
      if (typeof code === 'number') {
        resolve(code);
        return;
      }
      const signalNumber = signal ? osConstants.signals[signal] : undefined;
      resolve(typeof signalNumber === 'number' ? 128 + signalNumber : 1);
    });
  });
}

/**
 * Resolve a native binary for this platform and run it with this process's
 * arguments and stdio, then exit with its status.
 *
 * @param {string} binary native binary name without extension
 */
function launchNativeBinary(binary) {
  let binaryPath;
  try {
    binaryPath = resolveNativeBinaryPath(binary);
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
  runForwardingSignals(binaryPath, process.argv.slice(2)).then(
    code => process.exit(code),
    error => {
      console.error(`${binary}: failed to start binary: ${error.message}`);
      process.exit(1);
    }
  );
}

module.exports = { launchNativeBinary, runForwardingSignals, takeSignals };
