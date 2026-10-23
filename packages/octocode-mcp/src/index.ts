// `octocode-mcp` only starts the native binary's MCP server (`octocode mcp`):
// the binary owns the protocol, the tools and the configuration. Resolution
// and signal-forwarding are the CLI launcher's (one source for both packages).
import {
  NativeLaunchError,
  delegateToNative,
  resolveNativeBin,
} from '../../octocode/src/cli/native-delegate.js';
import { failStartup } from './startup-failure.js';

const resolved = resolveNativeBin();
if ('error' in resolved) {
  failStartup(`Server initialization failed: ${resolved.error}`);
} else {
  delegateToNative(resolved.bin, ['mcp', ...process.argv.slice(2)]).then(
    code => {
      process.exitCode = code;
    },
    (error: unknown) => {
      const message =
        error instanceof NativeLaunchError || error instanceof Error
          ? error.message
          : String(error);
      failStartup(`Server initialization failed: ${message}`);
    }
  );
}
