#!/usr/bin/env node

// bin/extract-hook-files.ts
var raw = "";
process.stdin.on("data", (chunk) => {
  raw += String(chunk);
});
process.stdin.on("end", () => {
  try {
    let add2 = function(value) {
      if (typeof value === "string" && value.trim()) {
        paths.push(value.trim());
      } else if (Array.isArray(value)) {
        for (const item of value) add2(item);
      }
    };
    var add = add2;
    const data = JSON.parse(raw);
    const toolInput = data !== null && typeof data === "object" && "tool_input" in data ? data.tool_input : null;
    const ti = toolInput !== null && typeof toolInput === "object" ? toolInput : {};
    const paths = [];
    add2(ti["file_path"]);
    add2(ti["path"]);
    add2(ti["file_paths"]);
    const command = ti["command"];
    if (typeof command === "string") {
      for (const line of command.split("\n")) {
        const addUpdDel = line.match(/^\*\*\* (?:Add|Update|Delete) File: (.+)$/);
        if (addUpdDel) {
          paths.push(addUpdDel[1].trim());
          continue;
        }
        const moveTo = line.match(/^\*\*\* Move to: (.+)$/);
        if (moveTo) paths.push(moveTo[1].trim());
      }
    }
    const seen = /* @__PURE__ */ new Set();
    for (const p of paths) {
      if (p && !seen.has(p)) {
        seen.add(p);
        process.stdout.write(p + "\n");
      }
    }
  } catch {
  }
  process.exit(0);
});
//# sourceMappingURL=extract-hook-files.js.map
