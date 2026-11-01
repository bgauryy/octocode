export { commandToken, defineCli, defineCommand, parseCommandInput } from './spec.js';
export type { CliCommand, CliSpec, JsonSchema } from './spec.js';
// commandHelp: agents-communication's interface test asserts its CLI's rendered help.
export { commandHelp } from './help.js';
export { cliView, runCli } from './run.js';
export type { CliIo } from './run.js';
export { cliFromMcp, connectStdio } from './from-mcp.js';
export { mcpServerOptions, registerOn } from './to-mcp.js';
