import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = fileURLToPath(new URL('../', import.meta.url));
const scripts = join(root, 'scripts');
const executable = `octocode-agents-communication${process.platform === 'win32' ? '.exe' : ''}`;
// Same platform table as the shipped launchers; rustc only when no prebuilt binary matches.
const launcherTargets = {
  'darwin-arm64': 'aarch64-apple-darwin', 'darwin-x64': 'x86_64-apple-darwin',
  'linux-arm64': 'aarch64-unknown-linux-gnu', 'linux-x64': 'x86_64-unknown-linux-gnu',
  'win32-x64': 'x86_64-pc-windows-msvc', 'win32-arm64': 'aarch64-pc-windows-msvc',
};
const prebuilt = target => join(scripts, 'bin', target, executable);
const mapped = launcherTargets[`${process.platform}-${process.arch}`];
const hostTarget = mapped && existsSync(prebuilt(mapped)) ? mapped
  : execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)[1];

/** Binary installed by src/build-skill.mjs for this host. */
export const nativeBinary = prebuilt(hostTarget);
/** COMMUNICATION_BINARY override, else the prebuilt host binary. */
export const binary = process.env.COMMUNICATION_BINARY || nativeBinary;
/** Shipped POSIX launcher. */
export const launcher = join(scripts, 'agents-communication');
/** COMMUNICATION_BINARY override, else the shipped launcher. */
export const launcherCommand = process.env.COMMUNICATION_BINARY || launcher;

/** Fresh temporary directory; `real` resolves symlinked temp roots such as /var → /private/var. */
export function tempDir(prefix, { real = false, parent = tmpdir() } = {}) {
  const directory = mkdtempSync(join(parent, prefix));
  return real ? realpathSync(directory) : directory;
}

/** tempDir removed after test `t`. */
export function tempWorkspace(t, prefix, options) {
  const directory = tempDir(prefix, options);
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  return directory;
}

/** `command '<json input>' --workspace W --database D [--session S]` argv builder. */
export const commandArgs = (workspace, database) => (command, input = {}, session) =>
  [command, JSON.stringify(input), '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])];

/** Synchronous JSON CLI call bound to one workspace and database. */
export function jsonCall(file, workspace, database, options = {}) {
  const args = commandArgs(workspace, database);
  return (command, input, session) => JSON.parse(execFileSync(file, args(command, input, session), { encoding: 'utf8', ...options }));
}

export const reasoningCommands = ['send_message', 'notify_all', 'lock', 'lock_many', 'share_document'];
/** Adds a default reasoning to commands that require one; explicit input wins. */
export const withReasoning = (command, input) => reasoningCommands.includes(command)
  ? { reasoning: `Verify ${command} behavior in this isolated regression fixture`, ...input }
  : input;
