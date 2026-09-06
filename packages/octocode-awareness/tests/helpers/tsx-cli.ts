import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);

export const tsxCli = require.resolve('tsx/cli');
