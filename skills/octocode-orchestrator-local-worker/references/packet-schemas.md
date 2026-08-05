# Packet Schemas

Load for example JSON schemas when building a worker packet.

### Summarize shard
```json
{
  "path": "string",
  "summary": "string (≤120 words)",
  "key_symbols": ["string"],
  "risks": ["string"],
  "confidence": "high|medium|low"
}
```

### Extract
```json
{
  "rows": [
    {
      "path": "string",
      "symbol": "string",
      "kind": "function|class|route|config",
      "notes": "string"
    }
  ],
  "unknowns": ["string"]
}
```

### Classify
```json
{
  "items": [
    {
      "id": "string",
      "label": "bug|chore|risk|question",
      "reason": "string (≤40 words)",
      "confidence": "high|medium|low"
    }
  ]
}
```

### Translate (often a small task)
```json
{
  "source_lang": "string",
  "target_lang": "string",
  "translation": "string",
  "notes": ["string"]
}
```

### Article / web-body summarize (already fetched)
```json
{
  "title": "string",
  "tldr": "string",
  "key_points": ["string"],
  "claims": [{ "claim": "string", "support_quote": "string" }],
  "confidence": "high|medium|low"
}
```
`support_quote` must be a verbatim contiguous substring of INPUT. Orchestrator verifies before integrate.