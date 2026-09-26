// DB handling can finish before the host emits its final usage. Wait for both.
export function nativeTurnFailure(vendor,events,session){
 if(vendor==='codex'){
  const failed=events.find(e=>e.method==='turn/completed'&&e.params?.threadId===session&&['failed','interrupted'].includes(e.params.turn?.status));
  if(failed){
   // Preserve classification without copying vendor error text that may contain credentials.
   const unauthorized=JSON.stringify(failed.params.turn.error??{}).includes('401');
   return `Codex native turn ${failed.params.turn.status}${unauthorized?' (HTTP 401 authentication)':''}; inspect the saved native events`;
  }
 }
 if(vendor==='claude'&&events.some(e=>e.type==='result'&&e.session_id===session&&e.is_error===true))return 'Claude native turn failed; inspect the saved native events';
 return null;
}
export function nativeTurnFinished(vendor,events,session){
 if(vendor==='claude'){
  const relevant=events.filter(e=>e.session_id===session);
  const lastResult=relevant.findLastIndex(e=>e.type==='result');
  const lastMessage=relevant.findLastIndex(e=>e.type==='assistant'||e.type==='user');
  return lastResult>=0&&lastResult>lastMessage&&relevant[lastResult].is_error===false&&Boolean(relevant[lastResult].usage);
 }
 if(vendor==='codex'){
  const turns=events.filter(e=>e.params?.threadId===session);
  const started=turns.filter(e=>e.method==='turn/started').map(e=>e.params.turn.id);
  return started.length>0&&started.every(id=>turns.some(e=>e.method==='turn/completed'&&e.params.turn.id===id&&e.params.turn.status==='completed'));
 }
 return false;
}
