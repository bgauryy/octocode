# Flow Analysis Protocol

> Tool descriptions and the funnel method (`SEARCH → LOCATE → TRACE → READ`) are available in the MCP server context. This file contains **review-specific tracing recipes** only.

## Flow Tracing Recipes (Local Repo)

### Recipe 1: "Who calls this modified function?"
```
1. localSearchCode(pattern="functionName") → get file + lineHint
2. lspCallHierarchy(symbolName="functionName", lineHint=N, direction="incoming") → list of callers
3. For each caller: localGetFileContent(matchString="callerName") → verify impact
```

### Recipe 2: "What does this new function call?"
```
1. localSearchCode(pattern="newFunction") → get lineHint
2. lspCallHierarchy(symbolName="newFunction", lineHint=N, direction="outgoing") → dependencies
3. For each dependency: lspGotoDefinition → verify contract
```

### Recipe 3: "All usages of this changed type/interface"
```
1. localSearchCode(pattern="TypeName") → get lineHint
2. lspFindReferences(symbolName="TypeName", lineHint=N) → all usages
3. For each usage in changed files: check compatibility
```

### Recipe 4: "Trace data flow A → B"
```
1. localSearchCode(pattern="entryPoint") → lineHint
2. lspCallHierarchy(direction="outgoing", depth=1) → first hop
3. For each hop: lspCallHierarchy(direction="outgoing", depth=1) → chain manually
4. localGetFileContent on critical nodes → verify transformations
```

---

## Flow Tracing Recipes (Remote Repo — github* tools only)

### Recipe 5: "Who calls this function?" (remote)
```
1. ghSearchCode(keywordsToSearch=["functionName"], owner=X, repo=Y, match="file") → find files
2. ghGetFileContent(matchString="functionName", matchStringContextLines=20) → see callers in context
3. Repeat for each file that imports/calls the function
```

### Recipe 6: "Trace import chain" (remote)
```
1. From diff: identify changed exports
2. ghSearchCode(keywordsToSearch=["import.*functionName"], match="file") → consumers
3. ghGetFileContent for each consumer → verify compatibility
```

---

## When to Use Which Recipe

| Changed Code | Recipe | Tools |
|-------------|--------|-------|
| Function signature changed | Recipe 1 (incoming callers) | `lspCallHierarchy(incoming)` or Recipe 5 |
| New function added | Recipe 2 (outgoing deps) | `lspCallHierarchy(outgoing)` |
| Type/Interface changed | Recipe 3 (all usages) | `lspFindReferences` or `ghSearchCode` |
| Data transformation changed | Recipe 4 (trace chain) | Chain `lspCallHierarchy` hops |
| Export changed | Recipe 6 (import chain) | `ghSearchCode` for consumers |
