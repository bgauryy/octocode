import {test} from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from './helpers.mjs';
import {rmSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {DatabaseSync} from 'node:sqlite';
import { nativeBinary as binary, tempDir, jsonCall } from './helpers.mjs';
function fixture(t){
 const workspace=tempDir('communication-stop-', { real: true }),database=join(workspace,'mail.sqlite');
 const call=jsonCall(binary,workspace,database,{stdio:'pipe',timeout:10000});
 const a=call('join',{name:'sender',vendor:'generic'}).id,b=call('join',{name:'recipient',vendor:'claude'}).id;
 call('attach',{transport:'claude',endpoint:join(workspace,'unused.sock'),vendorSession:'native-recipient'},b);
 const db=new DatabaseSync(database);t.after(()=>{db.close();rmSync(workspace,{recursive:true,force:true});});
 const event={hook_event_name:'Stop',stop_hook_active:false,session_id:'native-recipient',cwd:workspace};
 const send=(wake='action')=>call('send_message',{to:b,body:'DO NOT REPLAY THIS BODY',reasoning:'Complete requested review',replyRequired:wake==='action',wake},a).id;
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
 f.call('complete',{message:id},f.b);assert.deepEqual(f.call('completion-check',f.event,f.b),{});
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
 const body=full.replace(/^---\n[\s\S]*?\n---\n/,'');
 assert.ok(body!==full&&body.startsWith('# Agents communication'));
 for(const vendor of ['claude','codex','grok','pi','opencode','cursor','generic']) assert.equal(read(vendor),body);
});

test('Pi completion checks require bound raw identity and only list submitted pending IDs',t=>{
 const f=fixture(t),pi=f.call('join',{name:'pi',vendor:'pi',vendorSession:'pi-bound'}).id;
 f.call('attach',{transport:'raw',vendorSession:'pi-bound'},pi);
 const event={...f.event,session_id:'pi-bound'};
 const id=f.call('send_message',{to:pi,body:'passive body',reasoning:'Verify bounded recovery',wake:'passive'},f.a).id;
 assert.deepEqual(f.call('completion-check',event,pi),{});
 f.call('hook',{format:'json',consumer:'pi:pi-bound'},pi);
 const result=f.call('completion-check',event,pi);assert.deepEqual(result.pending,[id]);
 assert.ok(!result.reason.includes('passive body'));
 assert.throws(()=>f.call('completion-check',{...event,session_id:'other'},pi));
 assert.throws(()=>f.call('completion-check',{...event,cwd:tmpdir()},pi));
 f.call('leave',{},pi);assert.throws(()=>f.call('completion-check',event,pi));
});
