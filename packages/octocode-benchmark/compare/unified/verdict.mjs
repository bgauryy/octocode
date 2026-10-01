// Remove wording that reveals which tool set produced an answer.
const TOOL_WORDS = [
  'localSearch', 'localFetch', 'lspSearch', 'astSearch', 'structureSearch', 'artifactSearch', 'clasify',
  'ghSearchCode', 'ghGetFileContent', 'ghSearchHistory', 'ghGetHistoryItem', 'ghSearchRepo', 'ghStructure',
  'matchString', 'contextLines',
];
export function scrub(text) {
  let t = String(text ?? '');
  t = t.replace(/mcp__[\w-]+/g, '[tool]');
  t = t.replace(new RegExp(`\\b(${TOOL_WORDS.join('|')})\\b`, 'g'), '[tool]');
  t = t.replace(/\bnext\.\w+/g, '[tool]');
  t = t.replace(/\b[Oo]ctocode\b/g, '[tool]');
  t = t.replace(/\bripgrep\b/gi, '[tool]');
  t = t.replace(/\b(?:Bash|MCP|LSP|AST)\b/g, '[tool]');
  t = t.replace(/`(?:rg|gh|git)\s[^`\n]*`/g, '`[command]`');
  t = t.replace(/^(\s*\$?\s*)(?:rg|gh)\s.+$/gm, '$1[command]');
  t = t.replace(/\b(?:rg|gh)\b(?!\/)(?=[\s,.;:)])/g, '[tool]');
  return t;
}


const RANGES = { correctness: 5, completeness: 3, evidence: 2 };
export function parseVerdict(text) {
  const blocks = [...String(text).matchAll(/```json\s*([\s\S]*?)```/g)].map((m) => m[1]).reverse();
  for (const c of blocks) {
    try {
      const v = JSON.parse(c.trim());
      for (const k of ['X', 'Y']) {
        let sum = 0;
        for (const [dim, max] of Object.entries(RANGES)) {
          const s = v?.[k]?.[dim];
          if (typeof s !== 'number') throw new Error('numeric score required');
          if (!Number.isFinite(s) || s < 0 || s > max) throw new Error(`bad ${dim} for ${k}`);
          sum += s;
        }
        if (Number(v[k].quality) !== sum) { v[k].qualityAsWritten = v[k].quality; v[k].quality = sum; }
        v[k].wrong_claims ??= [];
      }
      if (!['X', 'Y', 'tie'].includes(v.preferred)) v.preferred = 'tie';
      return v;
    } catch { /* try the next block */ }
  }
  return null;
}

