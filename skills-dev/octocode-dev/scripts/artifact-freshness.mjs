// Freshness of staged native artifacts: each platform's `octocode` binary (CLI
// and MCP server) must embed the contract fingerprint and the release version they were
// published for. An artifact built before a contract change or a version bump
// still runs, so existence alone (check-outputs) does not catch it.
import fs from 'node:fs';
import path from 'node:path';

const ARTIFACT = /^octocode(\.exe)?$/;

/** Every staged binary under `npmRoot/<platform>/`. */
export function stagedNativeArtifacts(npmRoot) {
  if (!fs.existsSync(npmRoot)) return [];
  return fs
    .readdirSync(npmRoot, { withFileTypes: true })
    .filter(entry => entry.isDirectory())
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
