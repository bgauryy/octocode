'use strict';

const { spawn } = require('child_process');
const { constants: osConstants } = require('os');
const { resolveNativeBinaryPath } = require('./resolve-binary.cjs');

/**
 * Resolve a native binary for this platform and run it with this process's
 * arguments and stdio, then exit with its status.
 *
 * Async spawn (not spawnSync) so a signal directed only at the shim's PID
 * (e.g. SIGTERM from a supervisor, which does not hit the whole process group)
 * is forwarded to the native binary instead of queuing behind a blocking wait.
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

  const child = spawn(binaryPath, process.argv.slice(2), {
    stdio: 'inherit',
    windowsHide: false,
  });

  const forward = signal => {
    if (child.exitCode === null && child.signalCode === null) {
      child.kill(signal);
    }
  };
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
    process.on(signal, () => forward(signal));
  }

  child.on('error', error => {
    console.error(`${binary}: failed to start binary: ${error.message}`);
    process.exit(1);
  });

  child.on('close', (code, signal) => {
    if (typeof code === 'number') {
      process.exit(code);
    }
    // Signal death: report 128+N so OOM (SIGKILL→137) / crashes (SIGSEGV→139)
    // are distinguishable from an ordinary error exit.
    const signalNumber = signal ? osConstants.signals[signal] : undefined;
    process.exit(typeof signalNumber === 'number' ? 128 + signalNumber : 1);
  });
}

module.exports = { launchNativeBinary };
