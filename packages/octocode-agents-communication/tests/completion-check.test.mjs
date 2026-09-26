import {test} from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {mkdtempSync,rmSync,realpathSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {fileURLToPath} from 'node:url';
import {DatabaseSync} from 'node:sqlite';
const root=fileURLToPath(new URL('../',import.meta.url));
const target=execFileSync('rustc',['-vV'],{encoding:'utf8'}).match(/^host: (.+)$/m)[1];
const binary=join(root,'skills/octocode-agents-communication/scripts/bin',target,'octocode-agents-communication');
function fixture(t){
 const workspace=realpathSync(mkdtempSync(join(tmpdir(),'communication-stop-'))),database=join(workspace,'mail.sqlite');
 const call=(command,input={},session)=>JSON.parse(execFileSync(binary,[command,JSON.stringify(input),'--workspace',workspace,'--database',database,...(session?['--session',session]:[])],{encoding:'utf8',stdio:'pipe',timeout:10000}));
 const a=call('join',{name:'sender',vendor:'generic'}).id,b=call('join',{name:'recipient',vendor:'claude'}).id;
 call('attach',{transport:'claude',endpoint:join(workspace,'unused.sock'),vendorSession:'native-recipient'},b);
 const db=new DatabaseSync(database);t.after(()=>{db.close();rmSync(workspace,{recursive:true,force:true});});
 const event={hook_event_name:'Stop',stop_hook_active:false,session_id:'native-recipient',cwd:workspace};
 const send=(wake='action')=>call('send_message',{to:b,body:'DO NOT REPLAY THIS BODY',reasoning:'Complete requested review',wake},a).id;
 const offer=()=>{call('attach',{transport:'raw'},b);call('hook',{format:'json'},b);call('attach',{transport:'claude',endpoint:join(workspace,'unused.sock'),vendorSession:'native-recipient'},b);};
 return {call,a,b,db,event,send,workspace,offer};
}
test('Stop checks IDs only, once; never stages queued passive mail or ACKs work',t=>{
 const f=fixture(t),id=f.send('passive');const rows=()=>f.db.prepare('SELECT count(*) n FROM audit').get().n;
 const before=rows();assert.deepEqual(f.call('completion-check',f.event,f.b),{});assert.equal(rows(),before);
 // Explicit host offer in the fixture, no model or receiving daemon.
 f.offer();
 const offered=rows(),check=f.call('completion-check',f.event,f.b);
 assert.equal(check.decision,'block');assert.match(check.reason,new RegExp(`\\[${id}\\]`));
 assert.ok(!check.reason.includes('DO NOT REPLAY'));assert.equal(rows(),offered);
 assert.deepEqual(f.call('completion-check',{...f.event,stop_hook_active:true},f.b),{});
 assert.deepEqual(f.call('completion-check',{...f.event,hook_event_name:'StopFailure'},f.b),{});
 assert.equal(f.call('inbox',{message:id},f.b).items.length,1);
 assert.equal(f.db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=?').get(id).acknowledgedAt,null);
 f.call('ack',{message:id},f.b);assert.deepEqual(f.call('completion-check',f.event,f.b),{});
});
test('Stop enforces native identity/workspace and selective recovery cannot expose other mail',t=>{
 const f=fixture(t),first=f.send(),second=f.send();f.offer();
 assert.throws(()=>f.call('completion-check',{...f.event,session_id:'wrong'},f.b));
 assert.throws(()=>f.call('completion-check',{...f.event,cwd:tmpdir()},f.b));
 assert.deepEqual(f.call('inbox',{message:second},f.b).items.map(m=>m.id),[second]);
 assert.deepEqual(f.call('inbox',{message:first},f.a).items,[]);
 assert.throws(()=>f.call('inbox',{message:first,after:0},f.b));
 f.db.prepare('UPDATE messages SET expiresAt=0 WHERE id=?').run(first);
 assert.deepEqual(f.call('inbox',{message:first},f.b).items,[]);
 assert.ok(!f.call('completion-check',f.event,f.b).reason.includes(`[${first},`));
});
test('skill command returns the one installed routine for every vendor flag',()=>{
 const read=vendor=>JSON.parse(execFileSync(binary,['skill',...(vendor?['--vendor',vendor]:[])],{encoding:'utf8',timeout:10000})).instructions;
 const full=read();
 assert.match(full,/scripts\/agents-communication/);
 assert.doesNotMatch(full,/\*\*(Claude|Codex|Grok|OpenCode|Pi|Cursor\/Grok hooks):\*\*/);
 assert.doesNotMatch(full,/sqlite_agent/);
 for(const vendor of ['claude','codex','grok','pi','opencode','cursor','generic']) assert.equal(read(vendor),full);
});
