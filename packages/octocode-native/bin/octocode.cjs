#!/usr/bin/env node
/**
 * Platform-selecting bin for `@octocodeai/octocode-native` (e.g. `npx
 * @octocodeai/octocode-native`). Resolves the compiled binary via the shared
 * `resolve-binary.cjs` and spawns it, passing arguments and stdio through.
 *
 * The `octocode` npm CLI does NOT route through this launcher — it resolves the
 * platform binary directly (same resolver) and spawns it, so a tool call costs
 * one Node hop, not two.
 */
'use strict';

const { spawn } = require('child_process');
const { constants: osConstants } = require('os');
const { resolveNativeBinaryPath } = require('./resolve-binary.cjs');

// ── resolve platform binary ───────────────────────────────────────────────────

let binaryPath;
try {
  binaryPath = resolveNativeBinaryPath();
} catch (error) {
  console.error(error.message);
  process.exit(1);
}

// ── spawn ─────────────────────────────────────────────────────────────────────

// Async spawn (not spawnSync) so a signal directed only at this shim's PID
// (e.g. SIGTERM from a supervisor, which does not hit the whole process group)
// is forwarded to the native binary instead of queuing behind a blocking wait.
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
  console.error(`octocode: failed to start binary: ${error.message}`);
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
