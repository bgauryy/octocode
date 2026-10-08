import { realpathSync } from 'node:fs';
import { relative } from 'node:path';

// Canonicalize both sides: macOS /var and /private/var may identify the same workspace.
export const capturePath = (file: string, workspace: string) =>
  relative(realpathSync(workspace), realpathSync(file)) || '.';
