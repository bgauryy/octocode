'use strict';

const { spawnSync } = require('child_process');
const { randomUUID } = require('crypto');
const { chmodSync, closeSync, copyFileSync, openSync, readSync, renameSync, rmSync, statSync } = require('fs');

// Cold Darwin Mach-O validation can run for tens of seconds before user code.
const SMOKE_TIMEOUT_MS = process.platform === 'darwin' ? 60_000 : 20_000;

function sameFileBytes(left, right, size) {
  const leftFd = openSync(left, 'r');
  let rightFd;
  try {
    rightFd = openSync(right, 'r');
    const leftBuffer = Buffer.allocUnsafe(64 * 1024);
    const rightBuffer = Buffer.allocUnsafe(leftBuffer.length);
    for (let offset = 0; offset < size;) {
      const length = Math.min(leftBuffer.length, size - offset);
      const leftRead = readSync(leftFd, leftBuffer, 0, length, offset);
      const rightRead = readSync(rightFd, rightBuffer, 0, length, offset);
      if (leftRead !== length || rightRead !== length
          || !leftBuffer.subarray(0, length).equals(rightBuffer.subarray(0, length))) return false;
      offset += length;
    }
    return true;
  } finally {
    closeSync(leftFd);
    if (rightFd !== undefined) closeSync(rightFd);
  }
}

/**
 * Copy `source` to `destination` by replacing the inode, never overwriting it.
 * An in-place overwrite of a binary that a running process has mapped (the
 * CLI, or a live `octocode mcp` server) can crash that process or leave a
 * stale kernel code-signature cache that SIGKILLs later launches.
 */
function stageFile(source, destination, { executable = false } = {}) {
  const staged = `${destination}.${randomUUID()}.tmp`;
  try {
    copyFileSync(source, staged);
    if (executable && process.platform !== 'win32') chmodSync(staged, 0o755);
    // Replacing unchanged Mach-O bytes invalidates the OS validation cache.
    // Keep the verified inode; changed artifacts still replace it atomically.
    const existing = statSync(destination, { throwIfNoEntry: false });
    if (existing?.isFile()
        && existing.size === statSync(staged).size
        && (!executable || process.platform === 'win32' || (existing.mode & 0o777) === 0o755)
        && sameFileBytes(staged, destination, existing.size)) return;
    renameSync(staged, destination);
  } finally {
    rmSync(staged, { force: true });
  }
}

function verifyBinaryRuns(binaryPath) {
  const ran = spawnSync(binaryPath, ['--version'], { encoding: 'utf8', timeout: SMOKE_TIMEOUT_MS });
  if (ran.status !== 0) {
    throw new Error(
      `Binary smoke failed for ${binaryPath} (status ${ran.status}, signal ${ran.signal ?? 'none'}): ${ran.stderr || ran.stdout || ran.error?.message}`,
    );
  }
}

module.exports = { stageFile, verifyBinaryRuns };
