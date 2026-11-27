#!/usr/bin/env node
import { fileURLToPath } from 'node:url';
const args=process.argv.slice(2);
try {
  if(args[0]==='/raw') {
    process.argv=[process.execPath,fileURLToPath(new URL('../dist/engine/cli.mjs',import.meta.url)),...args.slice(1)];
    await import('../dist/engine/cli.mjs');
  } else {
    const cli=args[0]==='/cli'||['--help','-h','--version'].includes(args[0]);
    const entry=await import(cli?'../dist/cli.js':'../dist/mcp.js');
    process.exitCode=await (cli?entry.runChromeCli(args[0]==='/cli'?args.slice(1):args):entry.runChromeMcp(args[0]==='--mcp'?args.slice(1):args));
  }
} catch(error) {console.error(error.message);process.exitCode=error.exitCode??1;}
