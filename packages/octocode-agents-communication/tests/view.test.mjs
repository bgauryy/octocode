import {test} from 'node:test';
import assert from 'node:assert/strict';
import {get as httpGet} from 'node:http';
import {createConnection} from 'node:net';
import { spawn, execFileSync } from './helpers.mjs';
import {createInterface} from 'node:readline';
import {mkdirSync,existsSync} from 'node:fs';
import {join} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {binary,tempWorkspace,jsonCall} from './helpers.mjs';

async function fixture(t){
 const workspace=tempWorkspace(t,'communication-view-',{real:true}),database=join(workspace,'state.sqlite');
 const call=jsonCall(binary,workspace,database,{stdio:['pipe','pipe','pipe']});
 const a=call('join',{name:'Writer',vendor:'codex'}).id,b=call('join',{name:'Reviewer',vendor:'claude'}).id;
 const foreign=join(workspace,'foreign');mkdirSync(foreign);
 const other=jsonCall(binary,foreign,database,{stdio:['pipe','pipe','pipe']});
 other('join',{name:'OUTSIDE_WORKSPACE',vendor:'grok'});
 const child=spawn(binary,['view','{"open":false}', '--workspace',workspace,'--database',database],{stdio:['pipe','pipe','pipe']});
 let stderr='';child.stderr.on('data',d=>stderr+=d);
 const closed=new Promise(resolve=>child.once('exit',resolve));
 t.after(async()=>{if(child.exitCode===null)child.kill();await closed;});
 const ready=await new Promise((resolve,reject)=>{
  const timer=setTimeout(()=>reject(Error('View startup timeout: '+stderr)),15000);
  child.once('error',reject);child.once('exit',code=>{clearTimeout(timer);reject(Error('View exited '+code+': '+stderr));});
  createInterface({input:child.stdout}).once('line',line=>{clearTimeout(timer);resolve(JSON.parse(line));});
 });
 const get=async(path)=>{let response;try{response=await fetch(ready.url+path);}catch(cause){throw new Error('View fetch '+path+': '+stderr,{cause});}assert.equal(response.status,200);return response.json();};
 return {workspace,database,call,a,b,ready,get};
}

test('view exposes live workspace entities read-only, with bounded complete pagination',async t=>{
 const f=await fixture(t),db=new DatabaseSync(f.database);t.after(()=>db.close());
 for(let i=0;i<115;i++)f.call('send_message',{to:f.b,replyRequired:false,body:`message ${i} <script>not executable</script>`,conversationId:i<60?'older-thread':'recent-thread',reasoning:'Exercise observer pagination'},f.a);
 const request=f.call('send_message',{to:f.a,body:'Please review',reasoning:'Verify request and completion visibility'},f.b);
 const answer=f.call('complete',{message:request.id,reply:'Reviewed'},f.a);
 f.call('lock',{path:'active.rs',reasoning:'Owned edit',ttlMs:600000},f.a);
 const old=f.call('lock',{path:'expired.rs',reasoning:'Old edit',ttlMs:1000},f.b);
 db.prepare('UPDATE leases SET expiresAt=0 WHERE id=?').run(old.lease.id);
 f.call('share_document',{name:'handoff.md',content:'Evidence stays on disk',reasoning:'Shared handoff',context:{summary:'Review evidence',path:'active.rs'}},f.a);
 f.call('subscribe',{topics:['review']},f.a);
 f.call('attach',{transport:'raw'},f.a);
 const before={audit:db.prepare('SELECT count(*) n FROM records').get().n,sessions:db.prepare('SELECT count(*) n FROM sessions').get().n};
 const summary=await f.get('api/summary');assert.equal(summary.counts.activeAgents,2);assert.equal(summary.counts.activeLocks,1);
 const agents=await f.get('api/session');assert.equal(agents.items.length,2);assert.ok(agents.items.every(s=>s.name!=='OUTSIDE_WORKSPACE'));
 let page=await f.get('api/message'),ids=[];
 assert.equal(page.items.length,50);
 do{ids.push(...page.items.map(row=>row.id));page=page.next?await f.get('api/message?after='+page.next):null;}while(page);
 assert.equal(ids.length,117);assert.equal(new Set(ids).size,117);assert.deepEqual(ids,[...ids].sort((a,b)=>b-a));
 const recent=await f.get('api/message');assert.equal(recent.items[0].id,answer.id);assert.equal(recent.items[0].replyTo,request.id);assert.equal(recent.items[1].pending,0);
 assert.ok((await f.get('api/lease')).items.every(r=>r.path.endsWith('active.rs')));
 assert.equal((await f.get('api/document')).items[0].data.context.summary,'Review evidence');
 assert.deepEqual((await f.get('api/subscriptions?agent='+f.a)).items[0].topics,['review']);
 for(const entity of summary.entities){const value=await f.get('api/'+entity);assert.ok(Array.isArray(value.items));assert.ok(!JSON.stringify(value).includes('OUTSIDE_WORKSPACE'));}
 assert.equal((await f.get('api/delivery?agent='+f.a)).items.length,1);
 const deliveries=[];page=await f.get('api/delivery');do{deliveries.push(...page.items.map(r=>r.id));page=page.next?await f.get('api/delivery?after='+page.next):null;}while(page);
 assert.equal(deliveries.length,117);assert.equal(new Set(deliveries).size,117);
 assert.equal(db.prepare('SELECT count(*) n FROM records').get().n,before.audit);assert.equal(db.prepare('SELECT count(*) n FROM sessions').get().n,before.sessions);
 f.call('set_status',{status:'blocked',task:'Waiting for review'},f.a);
 assert.equal((await f.get('api/session?agent='+f.a)).items[0].status,'blocked','Existing observer reads fresh committed state');
 const oldest=await f.get('api/message?q='+encodeURIComponent('message 0 <script>'));
 assert.equal(oldest.items.length,1);assert.equal(oldest.items[0].body,'message 0 <script>not executable</script>');
 assert.ok(oldest.items[0].createdAt>0);assert.equal(oldest.items[0].senderVendor,'codex');
 assert.equal((await f.get('api/summary')).counts.messages,117);
 let thread=await f.get('api/message?conversation=older-thread');const threadIds=thread.items.map(r=>r.id);
 assert.equal(thread.items.length,50);assert.ok(thread.next);
 const tail=await f.get('api/message?conversation=older-thread&after='+thread.next);threadIds.push(...tail.items.map(r=>r.id));
 assert.equal(threadIds.length,60);assert.equal(new Set(threadIds).size,60);assert.equal(tail.next,null);
 assert.equal((await fetch(f.ready.url+'api/message?conversation=recent-thread&after='+thread.next)).status,400);
 assert.equal((await f.get('api/message?status=handled')).items.length,1);
 assert.ok((await f.get('api/message?status=pending')).items.every(r=>r.pending>0));
 assert.equal((await f.get('api/message?agent='+f.b+'&q='+encodeURIComponent('Please review'))).items.length,1,'Agent filter includes sent messages');
 assert.equal((await f.get('api/message?agent='+f.a+'&q='+encodeURIComponent('Please review'))).items.length,1,'Agent filter includes received messages');
 f.call('send_message',{to:f.b,replyRequired:false,body:'Résumé + 100% & history',reasoning:'UTF-8 literal search'},f.a);
 assert.equal((await f.get('api/message?q='+encodeURIComponent('Résumé + 100% &'))).items.length,1);
 assert.equal((await f.get('api/message?q='+encodeURIComponent("' OR 1=1 --"))).items.length,0);
 db.prepare('UPDATE sessions SET expiresAt=0 WHERE id=?').run(f.a);
 assert.equal((await f.get('api/message?q='+encodeURIComponent('message 0 <script>'))).items.length,1,'Past agents retain searchable history');
 assert.equal((await fetch(f.ready.url+'api/message?q=%FF')).status,400);
 assert.equal((await fetch(f.ready.url+'api/message?q=%GG')).status,400);
 assert.equal((await fetch(f.ready.url+'api/message?q=a&q=b')).status,400);
 assert.equal((await fetch(f.ready.url+'api/records?status=pending')).status,400);
 assert.equal((await fetch(f.ready.url+'api/message?status=unknown')).status,400);
 const wrongCursor=await fetch(f.ready.url+'api/records?after='+recent.next);assert.equal(wrongCursor.status,400);
 const html=await (await fetch(f.ready.url)).text();assert.ok(html.includes('Every agent. Every exchange.'));assert.ok(!html.includes('<script>not executable'));
});

test('view rejects mutations, foreign origins, invalid routes and missing databases',async t=>{
 const f=await fixture(t);
 const delayed=await new Promise((resolve,reject)=>{
  const url=new URL(f.ready.url);let response='';
  const socket=createConnection({host:url.hostname,port:Number(url.port)},()=>setTimeout(()=>socket.write(`GET ${url.pathname} HTTP/1.1\r\nHost: ${url.host}\r\nConnection: close\r\n\r\n`),100));
  socket.setTimeout(3000,()=>socket.destroy(Error('Delayed request timeout')));
  socket.on('data',chunk=>response+=chunk).on('error',reject).on('end',()=>resolve(response));
 });
 assert.match(delayed,/^HTTP\/1.1 200/,'Accepted sockets wait for request bytes within the bounded timeout');
 const forgedHost=await new Promise((resolve,reject)=>{httpGet(f.ready.url,{headers:{Host:'attacker.example'}},response=>{response.resume();resolve(response.statusCode);}).on('error',reject);});
 assert.equal(forgedHost,403);
 assert.equal((await fetch(f.ready.url+'api/session',{method:'POST'})).status,405);
 assert.equal((await fetch(f.ready.url+'api/session',{headers:{Origin:'https://example.com'}})).status,403);
 assert.equal((await fetch(new URL('/',f.ready.url))).status,404);
 assert.equal((await fetch(f.ready.url+'api/unknown')).status,400);
 assert.equal((await fetch(f.ready.url+'api/session?agent=invalid')).status,400);
 assert.equal((await fetch(f.ready.url+'api/session?sql=DROP')).status,400);
 const response=await fetch(f.ready.url);assert.equal(response.headers.get('cache-control'),'no-store');assert.equal(response.headers.get('x-frame-options'),'DENY');assert.ok(!response.headers.has('access-control-allow-origin'));
 const absent=join(f.workspace,'absent.sqlite');
 assert.throws(()=>execFileSync(binary,['view','{"open":false}','--workspace',f.workspace,'--database',absent],{stdio:'pipe'}));assert.equal(existsSync(absent),false);
});
