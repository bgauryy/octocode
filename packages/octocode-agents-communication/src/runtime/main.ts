/** The runnable entry: `node dist/communication.js <command> [json|-] [flags]`. */
import { main } from './cli.js';

process.exitCode = await main(process.argv.slice(2));
