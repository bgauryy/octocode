# Flow Analysis Protocol

> **Rule:** `localSearchCode` ALWAYS first → get real `lineHint` → then `lspGetSemantics`. Never guess `lineHint`.

---

## Local Repo Recipes

### Recipe 1: "Who calls this modified function?"
```
1. localSearchCode(pattern="functionName") → file + lineHint
2. lspGetSemantics(type="callers", symbolName="functionName", lineHint=N, format:"compact")
   → list of callers with file:line
3. For each caller: localGetFileContent(matchString="callerName") → verify impact
```

### Recipe 2: "What does this new function call?"
```
1. localSearchCode(pattern="newFunction") → lineHint
2. lspGetSemantics(type="callees", symbolName="newFunction", lineHint=N)
   → outgoing dependencies
3. For each dep: lspGetSemantics(type="definition") → verify contract
```

### Recipe 3: "All usages of a changed type/interface"
```
1. localSearchCode(pattern="TypeName") → lineHint
2. lspGetSemantics(type="references", symbolName="TypeName", lineHint=N, groupByFile:true)
   → per-file usage summary with line numbers
3. For each file in changed set: check compatibility
```

### Recipe 4: "Trace data flow A → B"
```
1. localSearchCode(pattern="entryPoint") → lineHint
2. lspGetSemantics(type="callHierarchy", symbolName, lineHint, depth=2, format:"compact")
   → full incoming+outgoing call tree
3. localGetFileContent on critical intermediate nodes → verify transformations
```

### Recipe 5: "Full blast radius of a function change"
```
1. localSearchCode(pattern="changedFn") → lineHint
2. lspGetSemantics(type="callHierarchy", lineHint, depth=3, format:"compact")
   → callers of callers
3. Note: callers result = cross-package; references = same-package. Use both.
```

---

## Remote Repo Recipes (github* tools only)

### Recipe 6: "Who calls this function?" (remote)
```
1. ghSearchCode(keywordsToSearch=["functionName"], owner=X, repo=Y, match="file")
   → candidate files
2. ghGetFileContent(matchString="functionName", contextLines=20)
   → callers in context
3. Repeat for each file that imports/calls the function
```

### Recipe 7: "Trace import chain" (remote)
```
1. From diff: identify changed exports
2. ghSearchCode(keywordsToSearch=["import.*functionName"], match="file")
   → consumers of the export
3. ghGetFileContent per consumer → verify compatibility
```

### Recipe 8: "Who introduced this pattern?" (archaeology)
```
1. ghHistoryResearch(type:"commits", path="file.ts", includeDiff:true)
   → find the commit that added the pattern
2. Extract PR number from messageHeadline (#N)
3. ghHistoryResearch(type:"prs", prNumber:N, content:{body:true, patches:{mode:"selected", files:["file.ts"]}})
   → original intent and context
```

---

## Recipe Selection Matrix

| Changed Code | Recipe | Primary Tool |
|-------------|--------|-------------|
| Function signature changed | Recipe 1 — incoming callers | `lspGetSemantics(type="callers")` |
| New function added | Recipe 2 — outgoing deps | `lspGetSemantics(type="callees")` |
| Type/Interface changed | Recipe 3 — all usages | `lspGetSemantics(type="references", groupByFile:true)` |
| Complex data flow | Recipe 4 — trace chain | `lspGetSemantics(type="callHierarchy", depth=2)` |
| High-risk change: full blast | Recipe 5 — deep blast | `lspGetSemantics(type="callHierarchy", depth=3)` |
| Remote function changed | Recipe 6 — remote callers | `ghSearchCode` + `ghGetFileContent` |
| Remote export changed | Recipe 7 — import chain | `ghSearchCode` for consumers |
| Why does this code exist? | Recipe 8 — archaeology | `ghHistoryResearch(type:"commits")` → PR |
