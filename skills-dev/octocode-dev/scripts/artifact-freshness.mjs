// Freshness of staged native artifacts: each platform's `octocode` binary (CLI
// and MCP server) must embed the contract fingerprint and the release version they were
// published for. An artifact built before a contract change or a version bump
// still runs, so existence alone (check-outputs) does not catch it.
import fs from 'node:fs';
import path from 'node:path';
import { getPlatformSuffix } from '../../../packages/octocode-native/bin/platform.cjs';

const ARTIFACT = /^octocode(\.exe)?$/;

/** The host's staged binaries; all platforms only when explicitly requested. */
export function stagedNativeArtifacts(npmRoot, { all = false } = {}) {
  const platform = all ? null : getPlatformSuffix();
  if (!all && !platform) throw new Error(`Unsupported native host ${process.platform}-${process.arch}`);
  if (!fs.existsSync(npmRoot)) return [];
  return fs
    .readdirSync(npmRoot, { withFileTypes: true })
    .filter(entry => entry.isDirectory() && (all || entry.name === platform))
    .flatMap(entry => {
      const dir = path.join(npmRoot, entry.name);
      return fs
        .readdirSync(dir)
        .filter(name => ARTIFACT.test(name))
        .map(name => path.join(dir, name));
    });
}

/** Artifacts that lack `fingerprint` or `version` in their bytes. */
export function staleArtifacts(artifacts, fingerprint, version) {
  return artifacts.filter(file => {
    const bytes = fs.readFileSync(file);
    return !bytes.includes(Buffer.from(fingerprint)) || !bytes.includes(Buffer.from(version));
  });
}
