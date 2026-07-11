/**
 * Barrel re-export for the remote-local materialization module.
 *
 * The implementation lives under `./remote-local/` (split for max-lines
 * compliance); this file exists so every other module in the repo can keep
 * importing from `../remote-local.js` unchanged.
 */
export type {
  RemoteMaterializationKind,
  RemoteLocationKind,
  RemoteLocation,
  RemoteMaterialization,
  RemoteMaterializationRequest,
} from './remote-local/types.js';

export { isFullRepoOption } from './remote-local/path-utils.js';

export {
  formatMaterializationHints,
  withMaterializationHints,
} from './remote-local/hints.js';

export { materializeRemoteForCli } from './remote-local/materialize.js';
