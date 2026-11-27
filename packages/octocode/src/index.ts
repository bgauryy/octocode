import { runCLI } from './cli/index.js';
import { errorMessage } from './cli/native-delegate.js';

runCLI().catch((error: unknown) => {
  console.error(errorMessage(error));
  process.exitCode = 5;
});
