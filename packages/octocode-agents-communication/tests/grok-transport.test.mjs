import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFile, execFileSync } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:net';
import { randomUUID } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
const root=fileURLToPath(new URL('../',import.meta.url));
const target=execFileSync('rustc',['-vV'],{encoding:'utf8'}).match(/^host: (.+)$/m)[1];
const binary=process.env.COMMUNICATION_BINARY??join(root,'skills/octocode-agents-communication/scripts/bin',target,'octocode-agents-communication');
const execute=promisify(execFile);
function fixture(t){
  const workspace=realpathSync(mkdtempSync('/tmp/octo-grok-test-')),database=join(workspace,'audit.sqlite');
  const args=(command,input={},session)=>[command,JSON.stringify(input),'--workspace',workspace,'--database',database,...(session?['--session',session]:[])];
  const call=(...input)=>JSON.parse(execFileSync(binary,args(...input),{encoding:'utf8',stdio:['pipe','pipe','pipe']}));
  const asyncCall=async(...input)=>JSON.parse((await execute(binary,args(...input),{timeout:15000})).stdout);
  const sender=call('join',{name:'sender',vendor:'raw'}),receiver=call('join',{name:'grok',vendor:'grok'}),vendorSession=randomUUID(),db=new DatabaseSync(database);
  t.after(()=>{db.close();rmSync(workspace,{recursive:true,force:true});});
  return{workspace,call,asyncCall,receiver,sender,vendorSession,db,send:(wake='action')=>call('send_message',{to:receiver.id,body:'Question requiring one answer',wake,key:randomUUID(),reasoning:'Verify existing Grok native delivery'},sender.id)};
}
async function leader(t,f,{mode='success',fragment=false,metadata={}}={}){
  const endpoint=join(f.workspace,'leader.sock'),requests=[],sockets=new Set();
  const frame=value=>{const body=Buffer.from(JSON.stringify(value)),size=Buffer.alloc(4);size.writeUInt32BE(body.length);return Buffer.concat([size,body]);};
  const server=createServer(socket=>{
    sockets.add(socket);socket.on('close',()=>sockets.delete(socket));socket.on('error',()=>{});let bytes=Buffer.alloc(0),queue=Promise.resolve();
    const send=value=>{const encoded=frame(value);queue=queue.then(async()=>{if(fragment){for(let i=0;i<encoded.length;i+=7){socket.write(encoded.subarray(i,i+7));await new Promise(r=>setTimeout(r,2));}}else socket.write(encoded);});};
    socket.on('data',chunk=>{bytes=Buffer.concat([bytes,chunk]);while(bytes.length>=4&&bytes.length>=4+bytes.readUInt32BE(0)){
      const size=bytes.readUInt32BE(0),message=JSON.parse(bytes.subarray(4,size+4));bytes=bytes.subarray(size+4);
      if(message.type==='register'){send({type:'registered',ready:true,leader_protocol_version:mode==='wrong-version'?2:1});continue;}
      if(message.type!=='acp')continue;const rpc=JSON.parse(message.payload);requests.push(rpc);
      const respond=result=>send({type:'acp',payload:JSON.stringify({jsonrpc:'2.0',id:rpc.id,result})});
      if(rpc.method==='initialize')respond({protocolVersion:mode==='wrong-acp'?2:1,agentCapabilities:{sessionCapabilities:{resume:{}}}});
      else if(rpc.method==='_x.ai/session/info')respond({result:mode==='missing-session'?{}:{sessionId:mode==='wrong-session'?randomUUID():f.vendorSession,cwd:mode==='wrong-workspace'?'/tmp':f.workspace}});
      else if(rpc.method==='session/prompt'){
        assert.equal(rpc.params.sessionId,f.vendorSession);assert.equal(rpc.params._meta.verbatim,true);assert.equal(rpc.params._meta.sendNow,undefined);assert.ok(rpc.params._meta.promptId);
        if(mode==='disconnect'){socket.destroy();continue;}
        if(mode==='oversized'){const huge=Buffer.alloc(4);huge.writeUInt32BE(8*1024*1024+1);socket.write(huge);continue;}
        if(mode==='rpc-error'){send({type:'acp',payload:JSON.stringify({jsonrpc:'2.0',id:rpc.id,error:{code:-32000,message:'fixture failed'}})});continue;}
        setTimeout(()=>respond({stopReason:'end_turn',_meta:{promptId:rpc.params._meta.promptId,...metadata}}),40);
      }
    }});
  });
  await new Promise(resolve=>server.listen(endpoint,resolve));t.after(()=>{for(const s of sockets)s.destroy();server.close();});
  f.call('attach',{transport:'grok',endpoint,vendorSession:f.vendorSession},f.receiver.id);return{requests,endpoint};
}
test('Grok targets a resident session without replacing its configuration, submits one batch, waits for completion and never acknowledges handling',{skip:process.platform==='win32'},async t=>{
 const f=fixture(t),native=await leader(t,f,{fragment:true});const sent=f.send();const result=await f.asyncCall('dispatch',{},f.receiver.id);
 assert.equal(result.submitted,1);assert.equal(result.recipientTurnRequested,true);assert.equal(result.modelCalls,0);
 assert.deepEqual(native.requests.map(r=>r.method),['initialize','_x.ai/session/info','session/prompt']);
 assert.equal(f.db.prepare('SELECT state FROM dispatches WHERE message=?').get(sent.id).state,'submitted');
 assert.equal(f.db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=?').get(sent.id).acknowledgedAt,null);
 assert.equal(f.call('dispatch',{},f.receiver.id).submitted,0);assert.equal(native.requests.length,3);
});
test('Grok passive messages remain in DB without connecting to the native endpoint',{skip:process.platform==='win32'},async t=>{
 const f=fixture(t),native=await leader(t,f);f.send('passive');assert.equal(f.call('dispatch',{},f.receiver.id).submitted,0);assert.equal(native.requests.length,0);assert.equal(f.db.prepare('SELECT count(*) n FROM dispatches').get().n,0);
});
test('Grok failed native offers remain uncertain and are never implicitly replayed',{skip:process.platform==='win32'},async t=>{
 for(const mode of ['disconnect','rpc-error','oversized'])await t.test(mode,async t=>{const f=fixture(t),native=await leader(t,f,{mode});const sent=f.send();await assert.rejects(()=>f.asyncCall('dispatch',{},f.receiver.id));assert.equal(f.db.prepare('SELECT state FROM dispatches WHERE message=?').get(sent.id).state,'uncertain');assert.equal(f.call('dispatch',{},f.receiver.id).submitted,0);assert.equal(native.requests.filter(r=>r.method==='session/prompt').length,1);});
});
test('Grok unsupported leader or ACP protocol fails before a prompt is sent',{skip:process.platform==='win32'},async t=>{
 for(const mode of ['wrong-version','wrong-acp','missing-session','wrong-session','wrong-workspace'])await t.test(mode,async t=>{const f=fixture(t),native=await leader(t,f,{mode});f.send();await assert.rejects(()=>f.asyncCall('dispatch',{},f.receiver.id));assert.equal(native.requests.filter(r=>r.method==='session/prompt').length,0);});
});
test('Grok validates a same-user existing socket and rejects path substitutions',{skip:process.platform==='win32'},async t=>{
 const f=fixture(t),native=await leader(t,f),file=join(f.workspace,'plain'),link=join(f.workspace,'link');writeFileSync(file,'not a socket');symlinkSync(native.endpoint,link);
 for(const endpoint of [file,link,join(f.workspace,'missing'),'relative.sock','ws://127.0.0.1:1234'])assert.throws(()=>f.call('attach',{transport:'grok',endpoint,vendorSession:f.vendorSession},f.receiver.id));
});
test('Grok records observed aggregate turn usage once per native batch, not per message or request',{skip:process.platform==='win32'},async t=>{
 const f=fixture(t);await leader(t,f,{metadata:{modelId:'grok-observed',inputTokens:3119,outputTokens:96,cachedReadTokens:2432,usage:{inputTokens:5647,outputTokens:174,cachedReadTokens:3584,cacheCreationTokens:0,modelCalls:2,numTurns:2}}});
 f.send();f.send();const result=await f.asyncCall('dispatch',{},f.receiver.id);assert.equal(result.submitted,2);
 const rows=f.db.prepare("SELECT key,data FROM audit WHERE kind='usage'").all();assert.equal(rows.length,1);
 const token=f.db.prepare('SELECT token FROM dispatches ORDER BY message LIMIT 1').get().token;
 assert.deepEqual(JSON.parse(rows[0].data),{key:`grok-${token}`,scope:'turn',model:'grok-observed',inputTokens:5647,outputTokens:174,cachedInputTokens:3584,cacheWriteTokens:0});
 assert.equal(f.call('dispatch',{},f.receiver.id).submitted,0);assert.equal(f.db.prepare("SELECT count(*) n FROM audit WHERE kind='usage'").get().n,1);
});
test('Grok missing or invalid usage remains unknown and cannot prevent native delivery',{skip:process.platform==='win32'},async t=>{
 for(const metadata of [{},{inputTokens:9,outputTokens:3},{usage:{inputTokens:-1,outputTokens:'7',cachedReadTokens:1.5,cacheCreationTokens:null}}])await t.test(JSON.stringify(metadata),async t=>{
  const f=fixture(t);await leader(t,f,{metadata});f.send();assert.equal((await f.asyncCall('dispatch',{},f.receiver.id)).submitted,1);assert.equal(f.db.prepare("SELECT count(*) n FROM audit WHERE kind='usage'").get().n,0);
 });
});
