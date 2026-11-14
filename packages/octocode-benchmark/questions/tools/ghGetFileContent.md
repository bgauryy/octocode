# ghGetFileContent: independently checkable questions

Use the [shared protocol](README.md) and [rating rubric](../../RATINGS.md).

| ID | Case | Question |
|---|---|---|
| GHF-01 | Normal | Read cli/cli README.md lines 1–25 at b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0, preserving exact source text and line identity. |
| GHF-02 | Advanced | Read the complete mergeConfig declaration in vitejs/vite packages/vite/src/node/utils.ts at f898888222297aa56d6fc281c585d4d4e4d51f58 with a small response window. Reconstruct all selected lines exactly once. Also inspect its outline and compact view and state their representation changes. |
| GHF-03 | Whole | Read the complete cli/cli internal/config/config.go at b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0 using a small response window, including all Unicode text. Compare reconstructed content against the immutable raw source. |
| GHF-04 | Empty | Look for octocode_benchmark_no_such_symbol_20261114 in cli/cli README.md at b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0; distinguish no matching source lines from an unreadable file. |
| GHF-05 | Invalid | Request endLine before startLine, omit path, and supply a sibling tool field. Retain rejections and corrected calls. |
| GHF-06 | Edge/source | Resolve jquery/jquery tag 3.7.1, then read the window around ajax in dist/jquery.min.js at the immutable resolved commit. Preserve complete selected minified evidence through every byte/response page. Read a small file at the tag and request a directory path to assess its actionable error. |
