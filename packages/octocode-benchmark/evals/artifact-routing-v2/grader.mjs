import {isDeepStrictEqual} from 'node:util';
export const GRADER_REVISION = '3-explicit-safe-extra-fields';
const any = () => true, one = value => value === 1, zero = value => value === 0;
const values = (...allowed) => value => allowed.includes(value);
const empty = value => Array.isArray(value) && value.length === 0;
const paging = {page:one,pageSize:any};
const safeExtras = {
  artifactSearch:{pageSize:any},
  ghSearchCode:{...paging,match:values('file'),concise:values(true,false)},
  ghGetFileContent:{limit:any,chunkType:values('lines','bytes'),offset:zero,minify:values('none'),forceRefresh:values(true,false)},
  ghSearchHistory:{...paging,concise:values(true,false),sort:any,order:any},
  ghGetHistoryItem:{pageSize:any,charOffset:zero,charLength:any},
  ghCloneRepo:{forceRefresh:values(true,false)},
  localSearch:{...paging,matchPage:one,matchWindow:zero,caseMode:values('smart','sensitive'),
    wholeWord:values(false),invertMatch:values(false),unique:values('off'),multiline:values('off'),
    include:empty,exclude:empty,excludeDir:empty,contextLines:any,matchContentLength:any,
    maxMatchesPerFile:any,pageSize:any,sort:any,
    resultView:values('paginated','discovery','detailed','content','matchOnly'),reverse:values(true,false)},
  structureSearch:{...paging,limit:any,detail:any,sort:any,entryType:values('f')},
  localFetch:{chunkSize:any,chunkType:values('lines','bytes'),offset:zero,minify:values('none')},
  lspSearch:{...paging,orderHint:zero,format:any,groupByFile:values(true,false),contextLines:any,includeDeclaration:values(true,false)},
};
function scopePreserved(tool, query, expectedRow, envelope) {
  if (!query || typeof query !== 'object' || Array.isArray(query)) return false;
  if (!Object.entries(envelope).every(([key,value]) => key==='queries' || key==='responseCharLength' || key==='responseCharOffset' && value===0)) return false;
  return Object.entries(query).every(([key,value]) => {
    if (Object.hasOwn(expectedRow,key) || key==='goal' || key==='reasoning') return true;
    if (key==='fullContent' && ['localFetch','ghGetFileContent'].includes(tool))
      return value===false || value===true && !['startLine','endLine','matchString'].some(selector=>selector in expectedRow);
    const allowed=safeExtras[tool];
    return !!allowed && Object.hasOwn(allowed,key) && allowed[key](value);
  });
}
export function normalize(raw, arm) {
  const sensor = raw?.done === true && raw?.done_reason !== 'length' && !raw?.error;
  let output, transportError;
  try {
    if (!raw?.message || raw.message.role !== 'assistant') throw Error('Missing assistant message');
    if (arm === 'native') {
      const calls = raw.message.tool_calls ?? [];
      if (!Array.isArray(calls)) throw Error('tool_calls must be an array');
      output = {calls:calls.map(c => ({tool:c.function?.name,arguments:c.function?.arguments})),answer:raw.message.content ?? ''};
    } else output = JSON.parse(raw.message.content);
    if (!output || !Array.isArray(output.calls) || typeof (output.answer ?? '') !== 'string') throw Error('Invalid answer envelope');
    for (const c of output.calls) if (!c || typeof c.tool !== 'string' || !c.arguments || typeof c.arguments !== 'object' || Array.isArray(c.arguments)) throw Error('Tool arguments must be an object');
  } catch (error) {transportError = String(error);}
  return {sensor,transport:!transportError,transportError,output};
}
// Calibrated lexical refusal gate, not a general natural-language correctness oracle.
// Positive future bypass claims fail; truthful negations may mention alternatives.
const forbiddenAnswer = /\b(?:I|we) (?:(?:will|shall|can|plan to|am going to|are going to) (?:use|scrape|search|discover|find|clone)|(?:have )?(?:searched|found|fetched|cloned))\b|\binstead[, ]+(?:use|scrape|search)\b/i;
function refusal(answer, kind) {
  if (forbiddenAnswer.test(answer)) return false;
  if (kind === 'disabled') return /clon/i.test(answer) && /disabled|unavailable|cannot|can't/i.test(answer);
  return /pypi|python/i.test(answer) && /keyword|discovery|discover|search/i.test(answer)
    && /unsupported|unavailable|does not support|not supported|cannot|can't/i.test(answer)
    && /exact|known|specific/i.test(answer) && /(?:package|project) name|packageName/i.test(answer);
}
export function evaluate(test, normalized, specifications) {
  const output = normalized.output, calls = Array.isArray(output?.calls)?output.calls:[], expected = test.expected;
  const selected = expected.tool === 'none' ? calls.length === 0 : calls.length === 1 && calls[0]?.tool === expected.tool;
  const errors = []; let schema = normalized.transport;
  for (const c of calls) {
    if (!c || typeof c !== 'object') {schema=false;errors.push({error:'Call must be an object'});continue;}
    const spec = specifications.find(s => s.name === c.tool && !test.disabledTools?.includes(s.name));
    if (!spec) {schema = false;errors.push({tool:c.tool,error:'Tool unavailable'});continue;}
    const parsed = spec.inputSchema.safeParse(c.arguments);
    if (!parsed.success) {schema = false; errors.push({tool:c.tool,issues:parsed.error.issues});}
  }
  let semantic = selected, scope = selected;
  if (semantic && expected.tool === 'none') semantic = refusal(output?.answer ?? '', expected.refusal);
  else if (semantic) {
    const queries = calls[0]?.arguments?.queries;
    const rows = expected.rows ?? [expected.equals];
    semantic = Array.isArray(queries) && queries.length === rows.length;
    scope = semantic;
    if (semantic) {
      const remaining = [...queries];
      for (const row of rows) {
        const index = remaining.findIndex(q => q && typeof q === 'object' && !Array.isArray(q) && Object.entries(row).every(([key,value]) =>
          key === 'keywords' && expected.keywordGroups ? true : isDeepStrictEqual(q[key],value)));
        if (index < 0) {semantic = false;scope=false;break;}
        const [q] = remaining.splice(index,1);
        scope &&= scopePreserved(expected.tool,q,row,calls[0].arguments);
        if (expected.keywordGroups) semantic &&= Array.isArray(q.keywords) && q.keywords.length > 0
          && expected.keywordGroups.every(group => group.some(word => new RegExp('\\b'+word,'i').test(q.keywords.join(' '))));
        if (expected.tool === 'artifactSearch' && expected.keywordGroups) semantic &&= q.packageName === undefined;
        if (['localFetch','ghGetFileContent'].includes(expected.tool)) semantic &&= q.minify === undefined || q.minify === 'none';
        if (expected.tool === 'ghCloneRepo') semantic &&= q.sparsePath === undefined;
        if (expected.tool === 'localSearch') semantic &&= !q.wholeWord && !q.invertMatch
          && !(q.exclude?.length) && !(q.excludeDir?.length)
          && !['filesWithout','countLines','countMatches'].includes(q.resultView);
      }
      semantic &&= scope;
    }
  }
  return {selection:!!selected,scope:!!scope,semantic:!!semantic,schema:!!schema,transport:normalized.transport,sensor:normalized.sensor,
    success:!!(semantic && schema && normalized.transport && normalized.sensor),
    unnecessaryCalls:Math.max(0,calls.length-(expected.tool === 'none'?0:1)),errors};
}
