// How to enable each evidence layer; shared by preflight, verify and check-report. Nothing here installs anything.
export const HINTS = {
  graph: 'Install Octocode (`npx -y octocode`) or build the local launcher `packages/octocode/out/octocode.js`; the graph needs `octocode graph`.',
  chrome: 'Install Chrome or Chromium, or set CHROME_PATH. Without it there is no page screenshot, DOM layout measurement or image review.',
  ast: 'astSearch needs the Octocode native engine; update Octocode. Without it edges are checked by text only.',
  ocr: 'Install Ollama (https://ollama.com), then `ollama pull glm-ocr` (2.7 GB; `glm-ocr:q8_0` is 1.8 GB for 8 GB machines). Without it the render is not read back by OCR; layout is still measured.',
  clasify: 'Enable clasify in the Octocode configuration (`octocode clasify \'{"queries":[{"operation":"models"}]}\'` must list a model). Without it leads are judged by AST and usage counts only.',
  history: 'Run inside a git checkout (not a frozen copy). Without it there are no hotspots or co-change facts.',
  lsp: 'Opt-in. Needs a language server (rust-analyzer, typescript-language-server, ...) and a warm start; pass `--layers ...,lsp`.',
  rsync: 'Install rsync. Only needed when the tree changes during ingest and a frozen copy is required.',
};
