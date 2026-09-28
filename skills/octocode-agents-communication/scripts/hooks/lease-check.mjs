// Optional host integrations share one read-only runtime admission call.
import {execFile} from 'node:child_process';
import {realpath} from 'node:fs/promises';
import {isAbsolute, relative, sep} from 'node:path';
import {promisify} from 'node:util';
import {runtimeCommand} from '../cli-command.mjs';
const execute = promisify(execFile);
// Hosts normalize aliases differently. Admit plain paths only instead of guessing
// whether lexical normalization and filesystem traversal identify the same file.
export function plainHostPath(path) {
  return typeof path === 'string' && path.length > 0 && !/[\0\u00A0\u2000-\u200A\u202F\u205F\u3000]/.test(path)
    && !/^[@~]|^file:/i.test(path) && !path.split(/[\\/]/).includes('..')
    && !(process.platform === 'win32' && (path.startsWith('/') || /^[a-z]:[^\\/]/i.test(path)));
}
export async function checkHostWrite(binding, {vendorSession, cwd, path}) {
  for (const key of ['binary', 'workspace', 'database', 'session']) {
    if (typeof binding?.[key] !== 'string' || !binding[key] || /[\r\n\0]/.test(binding[key])) throw Error('Missing host binding');
  }
  for (const key of ['binary', 'workspace', 'database']) if (!isAbsolute(binding[key])) throw Error('Host binding path must be absolute');
  if (typeof vendorSession !== 'string' || !vendorSession) throw Error('Missing native session');
  const workspace = await realpath(binding.workspace), directory = await realpath(cwd);
  const child = relative(workspace, directory);
  if (child === '..' || child.startsWith(`..${sep}`) || isAbsolute(child)) throw Error('Wrong workspace');
  if (!plainHostPath(path) || !plainHostPath(cwd)) throw Error('Use a plain path without host aliases or parent traversal');
  // The runtime owns canonical symlink and lease normalization for the plain target.
  const target = isAbsolute(path) ? path : `${directory}${sep}${path}`;
  const invocation = runtimeCommand(binding.binary, ['check_write', JSON.stringify({paths: [{path: target}], vendorSession}), '--workspace', workspace, '--database', binding.database, '--session', binding.session]);
  const {stdout} = await execute(invocation.command, invocation.args, {timeout: 2500, maxBuffer: 256 * 1024, windowsHide: true});
  const result = JSON.parse(stdout), expiry = result.checks?.[0]?.lease?.expiresAt;
  return result.ok === true && result.checks?.length === 1 && Number.isSafeInteger(expiry) && expiry > Date.now();
}
