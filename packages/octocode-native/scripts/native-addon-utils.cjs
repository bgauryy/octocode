'use strict';

const { spawnSync } = require('child_process');
const { randomUUID } = require('crypto');
const { chmodSync, closeSync, copyFileSync, openSync, readSync, renameSync, rmSync, statSync } = require('fs');
const { basename } = require('path');

// Cold Darwin Mach-O validation can run for tens of seconds before user code.
const SMOKE_TIMEOUT_MS = process.platform === 'darwin' ? 60_000 : 20_000;

function adHocSignDarwinAddon(artifactPath, platform, identifier = basename(artifactPath)) {
  if (!platform.startsWith('darwin')) return;

  const signed = spawnSync('codesign', ['--force', '--sign', '-', '--identifier', identifier, artifactPath], {
    encoding: 'utf8',
  });
  if (signed.status !== 0) {
    throw new Error(
      `Failed to ad-hoc sign ${artifactPath}: ${signed.stderr || signed.stdout}`,
    );
  }
}

/**
 * Copy `source` to `destination` by replacing the inode, never overwriting it.
 * An in-place overwrite of a Mach-O that a running process has mapped (the CLI,
 * or a `.node` addon loaded by a live MCP server) can crash that process or
 * leave a stale kernel code-signature cache that SIGKILLs later launches.
 * Darwin artifacts are ad-hoc signed before the rename, so the final path never
 * holds an unsigned file.
 */
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

function stageFile(source, destination, { platform, executable = false } = {}) {
  const staged = `${destination}.${randomUUID()}.tmp`;
  try {
    copyFileSync(source, staged);
    if (executable && process.platform !== 'win32') chmodSync(staged, 0o755);
    if (platform) adHocSignDarwinAddon(staged, platform, basename(destination));
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

function verifyAddonLoads(artifactPath) {
  const loaded = spawnSync(
    process.execPath,
    ['-e', 'require(process.argv[1])', artifactPath],
    { encoding: 'utf8', timeout: SMOKE_TIMEOUT_MS },
  );
  if (loaded.status !== 0) {
    throw new Error(
      `Native addon smoke failed for ${artifactPath} (status ${loaded.status}, signal ${loaded.signal ?? 'none'}): ${loaded.stderr || loaded.stdout || loaded.error?.message}`,
    );
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

module.exports = { SMOKE_TIMEOUT_MS, adHocSignDarwinAddon, stageFile, verifyAddonLoads, verifyBinaryRuns };
