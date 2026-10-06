import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { runtimeCommand } from '../scripts/cli-command.mjs';

export const digest = path => createHash('sha256').update(readFileSync(path)).digest('hex');
export const python = () => process.env.OCTOCODE_PYTHON || (process.platform === 'win32' ? 'python' : 'python3');
export function runRuntime(executable, args = [], options = {}) {
  const invocation = runtimeCommand(executable, args);
  return execFileSync(invocation.command, invocation.args, { encoding: 'utf8', timeout: 10000,
    killSignal: 'SIGKILL', maxBuffer: 2 * 1024 * 1024, stdio: ['pipe', 'pipe', 'pipe'], ...options });
}
export function runtimeInfo() {
  const value = JSON.parse(execFileSync(python(), ['-B', '-c', 'import sys,sqlite3,json; print(json.dumps({"implementation":"Python","version":sys.version.split()[0],"versionInfo":list(sys.version_info[:3]),"sqlite":sqlite3.sqlite_version}))'], { encoding: 'utf8', timeout: 10000 }));
  if (value.versionInfo[0] !== 3 || value.versionInfo[1] < 9) throw Error('Python 3.9 or newer is required.');
  return value;
}
export function checkSkill(executable, skillPath, timeoutMs = 10000) {
  const response = JSON.parse(runRuntime(executable, ['skill'], { timeout: timeoutMs }));
  if (response.instructions !== readFileSync(skillPath, 'utf8')) throw Error('Runtime skill differs from packaged OPERATING.md.');
  return { passed: true };
}
export function checkStartup(executable, timeoutMs = 10000, args = ['--help']) {
  const started = performance.now();
  let result;
  try { result = JSON.parse(runRuntime(executable, args, { timeout: timeoutMs })); }
  catch (error) { throw new Error(`Read-only startup verification failed for ${executable}; artifact not published (${error.code ?? error.message}).`, { cause: error }); }
  if (result?.package !== '@octocodeai/octocode-agents-communication' || result.implementation !== 'Python') throw Error(`Unexpected --help contract from ${executable}; artifact not published.`);
  return { passed: true, elapsedMs: performance.now() - started, timeoutMs };
}

export function payloadFiles(root) {
  return readdirSync(root, { withFileTypes: true }).sort((a,b) => a.name.localeCompare(b.name)).flatMap(entry => {
    if (entry.name === '__pycache__' || /(?:\.py[co]|\.tmp)$/.test(entry.name)) return [];
    if (entry.isSymbolicLink()) throw Error(`Portable runtime cannot contain a symlink: ${join(root, entry.name)}`);
    const path = join(root, entry.name);
    return entry.isDirectory() ? payloadFiles(path) : entry.isFile() ? [path] : [];
  });
}
export function payloadDigest(root) {
  const hash = createHash('sha256');
  for (const path of payloadFiles(root)) hash.update(relative(root, path).replaceAll('\\', '/') + '\0').update(readFileSync(path)).update('\0');
  return hash.digest('hex');
}
