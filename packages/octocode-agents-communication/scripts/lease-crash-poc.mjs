import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createInterface } from 'node:readline';

// Actual SIGKILL and wall-clock presence expiry. No fixture timestamp edits.
const probeInput=(command,input)=>['send_message','notify_all','lock','lock_many'].includes(command)?{reasoning:`Validate ${command} interoperability in this isolated communication exercise`,...input}:input;

const root=fileURLToPath(new URL('../',import.meta.url));
const cli=join(root,'skills/octocode-agents-communication/scripts/agents-communication');
const workspace=mkdtempSync(join(tmpdir(),'communication-crash-')),database=join(workspace,'v1.sqlite');
const invoke=(name,input={},session)=>JSON.parse(execFileSync(cli,[...name.split(' '),...(input===null?[]:[JSON.stringify(probeInput(name,input))]),'--workspace',workspace,'--database',database,...(session?['--session',session]:[])],{encoding:'utf8',stdio:['pipe','pipe','pipe']}));
const owner=invoke('join',{name:'killed-listener',vendor:'raw'}),peer=invoke('join',{name:'recovery-peer',vendor:'raw'});
const lease=invoke('lock',{path:'crashed.md',ttlMs:120000},owner.id).lease;
invoke('attach',{transport:'raw'},owner.id);
let heartbeatError;
const heartbeat=setInterval(()=>{try{invoke('heartbeat',{},peer.id);}catch(error){heartbeatError=error;}},10000);
const child=spawn(cli,['listen','--workspace',workspace,'--database',database,'--session',owner.id],{stdio:['ignore','pipe','pipe']});
const lines=createInterface({input:child.stdout});
let stderr='';child.stderr.on('data',chunk=>stderr+=chunk);
try {
 await new Promise((resolve,reject)=>{
  const timer=setTimeout(()=>reject(Error('Listener startup timed out')),10000);
  child.once('error',reject);child.once('exit',()=>reject(Error(stderr||'Listener exited before ready')));
  lines.on('line',line=>{try{if(JSON.parse(line).type==='listening'){clearTimeout(timer);resolve();}}catch(error){clearTimeout(timer);reject(error);}});
 });
 const killedAt=Date.now();
 await new Promise(resolve=>{child.once('exit',resolve);child.kill('SIGKILL');});
 assert.equal(invoke('lock',{path:'crashed.md'},peer.id).ok,false,'do not steal before expiry');
 const presence=invoke('entity get session '+owner.id,null,peer.id);
 console.log(JSON.stringify({phase:'listener killed; waiting for real presence expiry',workspace,expiresAt:presence.expiresAt}));
 while(Date.now()<=presence.expiresAt){if(heartbeatError)throw heartbeatError;await new Promise(resolve=>setTimeout(resolve,Math.min(1000,presence.expiresAt-Date.now()+25)));}
 const recovered=invoke('lock',{path:'crashed.md'},peer.id);
 assert.equal(recovered.ok,true);assert.ok(Date.now()<lease.expiresAt,'owner presence, not lease TTL, freed the path');
 assert.throws(()=>invoke('renew',{lease:lease.id},owner.id));
 invoke('resume',{vendor:'raw'},owner.id);assert.equal(invoke('renew',{lease:lease.id},owner.id).renewed,false);
 invoke('unlock',{lease:recovered.lease.id},peer.id);
 const result={passed:true,workspace,database,killedPid:child.pid,recoveryMs:Date.now()-killedAt,leaseTtlMs:120000,presenceTtlMs:60000,checks:['real SIGKILL','conflict retained before expiry','natural wall-clock owner expiry frees longer lease','expired owner cannot renew','resume cannot revive stale ID']};
 mkdirSync(join(root,'out'),{recursive:true});writeFileSync(join(root,'out/lease-crash-poc.json'),JSON.stringify(result,null,2)+'\n');
 console.log(JSON.stringify(result));
} finally {
 clearInterval(heartbeat);lines.close();if(child.exitCode===null&&child.signalCode===null)child.kill('SIGKILL');
 invoke('leave',{},owner.id);invoke('leave',{},peer.id);
}
