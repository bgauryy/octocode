'use strict';

const { spawnSync } = require('child_process');

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

module.exports = { adHocSignDarwinAddon, verifyAddonLoads };
