'use strict';

const { spawnSync } = require('child_process');
const { randomUUID } = require('crypto');
const { chmodSync, copyFileSync, renameSync, rmSync } = require('fs');

function adHocSignDarwinAddon(artifactPath, platform) {
  if (!platform.startsWith('darwin')) return;

  const signed = spawnSync('codesign', ['--force', '--sign', '-', artifactPath], {
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
function stageFile(source, destination, { platform, executable = false } = {}) {
  const staged = `${destination}.${randomUUID()}.tmp`;
  try {
    copyFileSync(source, staged);
    if (executable && process.platform !== 'win32') chmodSync(staged, 0o755);
    if (platform) adHocSignDarwinAddon(staged, platform);
    renameSync(staged, destination);
  } finally {
    rmSync(staged, { force: true });
  }
}

function verifyAddonLoads(artifactPath) {
  const loaded = spawnSync(
    process.execPath,
    ['-e', 'require(process.argv[1])', artifactPath],
    { encoding: 'utf8', timeout: 20_000 },
  );
  if (loaded.status !== 0) {
    throw new Error(
      `Native addon smoke failed for ${artifactPath} (status ${loaded.status}, signal ${loaded.signal ?? 'none'}): ${loaded.stderr || loaded.stdout}`,
    );
  }
}

function verifyBinaryRuns(binaryPath) {
  const ran = spawnSync(binaryPath, ['--version'], { encoding: 'utf8', timeout: 20_000 });
  if (ran.status !== 0) {
    throw new Error(
      `Binary smoke failed for ${binaryPath} (status ${ran.status}, signal ${ran.signal ?? 'none'}): ${ran.stderr || ran.stdout || ran.error?.message}`,
    );
  }
}

module.exports = { adHocSignDarwinAddon, stageFile, verifyAddonLoads, verifyBinaryRuns };
