// The postinstall entry, bundled to out/install.js: make the installed
// `octocode` bin the native binary (see cli/native-install.ts). An install
// never fails here; when the bin stays the Node launcher for a reason this
// install context does not explain, one stderr line says why.
import { fileURLToPath } from 'node:url';
import { bundledNativeVersion } from './cli/native-delegate.js';
import { installNativeBin, installNotice } from './cli/native-install.js';

const outcome = installNativeBin({
  packageRoot: fileURLToPath(new URL('..', import.meta.url)),
  version: bundledNativeVersion,
});
const notice = installNotice(outcome);
if (notice) process.stderr.write(`${notice}\n`);
