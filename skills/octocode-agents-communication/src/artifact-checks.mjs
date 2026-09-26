import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, copyFileSync, readFileSync, renameSync, rmSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export const rustHostTarget = () => execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)[1];
export const executableName = target => `octocode-agents-communication${target.includes('windows') ? '.exe' : ''}`;
/** Executable that build-skill.mjs installs for `target`. */
export const installedBinary = (target = rustHostTarget()) =>
  fileURLToPath(new URL(`../scripts/bin/${target}/${executableName(target)}`, import.meta.url));
export const digest = path => createHash('sha256').update(readFileSync(path)).digest('hex');

export function checkSkill(executable, skillPath, timeoutMs = 10000) {
  const response = JSON.parse(execFileSync(executable, ['skill'], {
    encoding: 'utf8', timeout: timeoutMs, killSignal: 'SIGKILL', maxBuffer: 1024 * 1024,
    stdio: ['ignore', 'pipe', 'pipe'],
  }));
  if (response.instructions !== readFileSync(skillPath, 'utf8')) {
    throw new Error('Embedded skill differs from packaged SKILL.md; rebuild before packaging.');
  }
  return { passed: true };
}

export function checkStartup(executable, timeoutMs = 10000) {
  const started = performance.now();
  let result;
  try {
    result = JSON.parse(execFileSync(executable, ['--help'], {
      encoding: 'utf8', timeout: timeoutMs, killSignal: 'SIGKILL', maxBuffer: 1024 * 1024,
      stdio: ['ignore', 'pipe', 'pipe'],
    }));
  } catch (error) {
    throw new Error(`Read-only startup verification failed for ${executable}; artifact not published (${error.code ?? error.message}).`, { cause: error });
  }
  if (result?.package !== '@octocodeai/octocode-agents-communication' || result.implementation !== 'Rust') {
    throw new Error(`Unexpected --help contract from ${executable}; artifact not published.`);
  }
  return { passed: true, elapsedMs: performance.now() - started, timeoutMs };
}

export function verifyExecutable(executable, { target, hostTarget, timeoutMs = 10000 }) {
  let signature = 'not-checked-on-this-host';
  if (process.platform === 'darwin' && target.endsWith('-apple-darwin')) {
    execFileSync('/usr/bin/codesign', ['--verify', '--strict', executable], {
      timeout: timeoutMs, killSignal: 'SIGKILL', stdio: ['ignore', 'pipe', 'pipe'],
    });
    signature = 'valid';
  }
  return { signature, startup: target === hostTarget ? checkStartup(executable, timeoutMs) : { passed: null, reason: 'foreign-target-needs-native-CI' } };
}

export function installExecutable(source, destination, options) {
  const sourceHash = digest(source);
  let unchanged = false;
  try { unchanged = digest(destination) === sourceHash; }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  if (unchanged) {
    if ((statSync(destination).mode & 0o777) !== 0o755) chmodSync(destination, 0o755);
    return { changed: false, sha256: sourceHash, verification: verifyExecutable(destination, options) };
  }
  const temporary = `${destination}.${process.pid}.tmp`;
  try {
    copyFileSync(source, temporary);
    chmodSync(temporary, 0o755);
    const verification = verifyExecutable(temporary, options);
    renameSync(temporary, destination);
    return { changed: true, sha256: sourceHash, verification };
  } finally { rmSync(temporary, { force: true }); }
}
