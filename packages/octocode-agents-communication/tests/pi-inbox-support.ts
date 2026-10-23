// The fake Pi host the Pi inbox tests drive: a temporary workspace and database, sent messages and a durable ledger.
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { JsonRecord } from './json-record.ts';
import { binary, tempWorkspace, jsonCall, fakePi } from './helpers.ts';
import type { PiContext, PiMessage, PiSendOptions } from '../src/hosts/shared/command.ts';

/** A message the fake Pi host received and its send options. */
export interface Sent { message: PiMessage; options: PiSendOptions }
/** The details a sent message must carry. */
export const detailsOf = (sent: Sent): JsonRecord => { assert.ok(sent.message.details, 'the message carries details'); return sent.message.details; };
export type Fixture = ReturnType<typeof fixture>;
/** A fake Pi host over a temporary workspace and database, with an in-memory ledger mirrored to its session file. */
export function fixture(t: TestContext) {
 const workspace = tempWorkspace(t, 'pi-ledger-'),database=join(workspace,'db.sqlite');
 const run=jsonCall(binary,workspace,database);
 const ledger: JsonRecord[]=[],sent: Sent[]=[]; let persist=true,disk=true;
 const sessionFile=join(workspace,'pi-session.jsonl');
 const flush=()=>{if(disk)writeFileSync(sessionFile,[JSON.stringify({type:'session',id:'pi-test'}),...ledger.map((e: JsonRecord)=>JSON.stringify(e))].join('\n')+'\n');};
 const {host: pi,handlers,tools}=fakePi((message,options)=>{sent.push({message,options});if(persist){ledger.push({type:'custom_message',...message});flush();}});
 const ctx: PiContext={cwd:workspace,sessionManager:{getSessionId:()=> 'pi-test',getSessionFile:()=>sessionFile,getEntries:()=>ledger}};
 const fire=async (name: string,event: JsonRecord={},context: PiContext=ctx)=>handlers.get(name)?.(event,context);
 return {workspace,database,run,ledger,tools,sent,pi,ctx,fire,flush,setDisk:(value: boolean)=>{disk=value;},setPersist:(value: boolean)=>{persist=value;},tool:(name: string)=>{const found=tools.find(candidate=>candidate.name===name);assert.ok(found,`tool ${name} is registered`);return found;}};
}
