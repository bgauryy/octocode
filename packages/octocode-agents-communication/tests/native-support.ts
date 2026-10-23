// The black-box CLI fixture of the native tests: a temporary workspace and database, and synchronous and spawned CLI calls.
import type { TestContext } from 'node:test';
import { rmSync, symlinkSync } from 'node:fs';
import { join } from 'node:path';
import type { JsonRecord } from './json-record.ts';
import { binary, execFileSync, reasoningCommands, root, spawn, tempWorkspace, withReasoning } from './helpers.ts';

export type Fixture = ReturnType<typeof fixture>;

/** CLI arguments with the reasoning field a reasoning command requires added to its JSON input. */
function testArgs(args: string[]){
 if(!reasoningCommands.includes(args[0])||!args[1]?.startsWith('{'))return args;
 return [args[0],JSON.stringify(withReasoning(args[0],JSON.parse(args[1]))),...args.slice(2)];
}

export const skill=join(root,'.');
export function stableVendor(bin: string,vendor: string,script: string){rmSync(join(bin,vendor),{force:true});symlinkSync(join(root,'tests/vendor-fixture.sh'),join(bin,vendor));return {COMMUNICATION_FIXTURE_NODE:process.execPath,COMMUNICATION_FIXTURE_SCRIPT:script};}
/** A temporary workspace and a database path in a not-yet-created subdirectory. */
export function fixture(t: TestContext){const workspace = tempWorkspace(t, 'communication-native-');return {workspace,database:join(workspace,'nested/communication.sqlite')};}
export function invoke(context: Fixture,args: string[],extra: Record<string, unknown>={}): JsonRecord{return JSON.parse(execFileSync(binary,[...testArgs(args),'--workspace',context.workspace,'--database',context.database],{encoding:'utf8',stdio:['pipe','pipe','pipe'],...extra}));}
export function joinAgent(context: Fixture,name='a'){return invoke(context,['join',JSON.stringify({name,vendor:'generic'})]);}
export function start(context: Fixture,args: string[],input?: string){return new Promise<{code: number|null;stdout: string;stderr: string}>((resolve,reject)=>{const child=spawn(binary,[...testArgs(args),'--workspace',context.workspace,'--database',context.database]);let stdout='',stderr='';child.stdout.on('data',v=>stdout+=v);child.stderr.on('data',v=>stderr+=v);child.on('error',reject);child.on('close',code=>resolve({code,stdout,stderr}));child.stdin.end(input);});}
