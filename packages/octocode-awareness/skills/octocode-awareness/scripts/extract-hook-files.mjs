#!/usr/bin/env node

// bin/extract-hook-files.ts
var USAGE = `usage: extract-hook-files < hook-payload.json

Reads a hook JSON payload from stdin and prints one deduplicated file path per line.
Supports Claude tool_input, Cursor file_path, Pi input/args, and Codex apply_patch command payloads.
`;
if (process.argv.includes("--help") || process.argv.includes("-h")) {
  process.stdout.write(USAGE);
  process.exit(0);
}
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
    }, addTargets2 = function(source) {
      add2(source["file_path"]);
      add2(source["path"]);
      add2(source["filePath"]);
      add2(source["paths"]);
      add2(source["file_paths"]);
      add2(source["filePaths"]);
      const queries = source["queries"];
      if (Array.isArray(queries)) {
        for (const query of queries) {
          if (!query || typeof query !== "object") continue;
          addTargets2(query);
        }
      }
    };
    var add = add2, addTargets = addTargets2;
    let data;
    try {
      data = JSON.parse(raw);
    } catch {
      data = raw;
    }
    const root = data !== null && typeof data === "object" ? data : {};
    const toolInput = root.tool_input ?? root.input ?? root.args ?? data;
    const ti = toolInput !== null && typeof toolInput === "object" ? toolInput : {};
    const paths = [];
    addTargets2(root);
    if (ti !== root) addTargets2(ti);
    const command = typeof toolInput === "string" ? toolInput : ti["command"] ?? root["command"] ?? ti["patch"] ?? root["patch"] ?? ti["text"] ?? root["text"] ?? ti["content"] ?? root["content"];
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
