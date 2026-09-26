import {isAbsolute} from 'node:path';
import {checkHostWrite} from './lease-check.mjs';

// Deliberate factory: importing the module alone never installs a host hook.
export function createOpenCodeLeaseGuard(options) {
  const bindings = Object.freeze({...options?.sessions});
  const fixed = Object.freeze({binary: options?.binary, workspace: options?.workspace, database: options?.database});
  return async ({directory}) => ({
    'tool.execute.before': async (input, output) => {
      if (!['write', 'edit'].includes(input.tool)) return;
      const nativeSession = input.sessionID, path = output?.args?.filePath;
      const session = Object.hasOwn(bindings, nativeSession) ? bindings[nativeSession] : undefined;
      try {
        if (typeof path !== 'string' || !isAbsolute(path)) throw Error('OpenCode structured file tools require absolute paths');
        const covered = await checkHostWrite({...fixed, session}, {vendorSession: nativeSession, cwd: directory, path});
        if (!covered || input.sessionID !== nativeSession || output?.args?.filePath !== path) throw Error('Uncovered or changed input');
      } catch {
        throw Error('File edit blocked: live communication lease ownership or the host session binding could not be verified. Acquire or renew your own covering lease, then retry.');
      }
    },
  });
}
