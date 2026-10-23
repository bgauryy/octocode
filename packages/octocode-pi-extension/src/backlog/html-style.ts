/** The backlog page's style (`html.ts` places it inline). */

export const PAGE_CSS = String.raw`
:root {
  color-scheme: light dark;
  --bg: #f6f7f9; --panel: #ffffff; --text: #1c2024; --muted: #5b6672; --line: #d9dee4; --accent: #2457d6;
  --ongoing: #1f7a3d; --todo: #2457d6; --backlog: #8a5a00; --done: #5b6672; --warn: #b3261e;
  --chip: #eceff3; --note: #f1f3f6; --change: #e8eefc;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #14171a; --panel: #1c2024; --text: #e6e9ec; --muted: #9aa5b1; --line: #343b43; --accent: #7aa2ff;
    --ongoing: #5ccf84; --todo: #7aa2ff; --backlog: #e0b050; --done: #9aa5b1; --warn: #ff8a80;
    --chip: #2a3037; --note: #232830; --change: #1f2a44;
  }
}
* { box-sizing: border-box; }
[hidden] { display: none !important; }
body { margin: 0; background: var(--bg); color: var(--text); font: 14px/1.45 system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif; }
a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }
header.top { padding: 14px 20px 0; background: var(--panel); border-bottom: 1px solid var(--line); }
h1 { margin: 0 0 2px; font-size: 20px; overflow-wrap: anywhere; }
.snapshot { color: var(--muted); font-size: 12px; }
.stats { display: flex; flex-wrap: wrap; gap: 6px 14px; margin: 10px 0; }
.stat { color: var(--muted); }
.stat b { color: var(--text); font-weight: 600; }
nav.tabs { display: flex; gap: 2px; margin-top: 6px; }
nav.tabs a { padding: 8px 14px; color: var(--muted); border-bottom: 2px solid transparent; }
nav.tabs a.active { color: var(--text); border-bottom-color: var(--accent); font-weight: 600; }
nav.tabs a:hover { text-decoration: none; color: var(--text); }
.badge { margin-left: 6px; padding: 0 6px; border-radius: 9px; background: var(--warn); color: #fff; font-size: 11px; }
main { padding: 16px 20px 60px; }
.chip { display: inline-block; padding: 0 7px; border-radius: 9px; background: var(--chip); color: var(--muted); font-size: 11px; line-height: 18px; white-space: nowrap; }
.chip.p0 { background: var(--warn); color: #fff; }
.chip.p1 { color: var(--warn); }
.chip.ongoing { color: var(--ongoing); } .chip.todo { color: var(--todo); } .chip.backlog { color: var(--backlog); } .chip.done { color: var(--done); }
.chip.stale { background: var(--warn); color: #fff; }
.ref { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-weight: 600; }
.board { display: grid; grid-template-columns: repeat(auto-fit, minmax(250px, 1fr)); gap: 14px; align-items: start; }
.column { min-width: 0; }
.column > h2 { margin: 0 0 8px; font-size: 13px; text-transform: uppercase; letter-spacing: .04em; color: var(--muted); display: flex; justify-content: space-between; align-items: center; }
.column > h2 button { font: inherit; font-size: 12px; text-transform: none; letter-spacing: 0; }
.card { display: block; margin-bottom: 8px; padding: 9px 11px; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; color: var(--text); }
.card:hover { text-decoration: none; border-color: var(--accent); }
.card .top { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
.card .title { margin: 4px 0; overflow-wrap: anywhere; }
.card .meta { color: var(--muted); font-size: 12px; display: flex; gap: 4px 10px; flex-wrap: wrap; overflow-wrap: anywhere; }
.card.done .title { color: var(--muted); }
.empty { color: var(--muted); padding: 6px 2px; }
.filters { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 10px; align-items: center; }
.filters input[type=search] { min-width: 260px; }
input, select, button { font: inherit; color: var(--text); background: var(--panel); border: 1px solid var(--line); border-radius: 5px; padding: 4px 8px; }
button { cursor: pointer; }
table { width: 100%; border-collapse: collapse; background: var(--panel); border: 1px solid var(--line); }
th, td { text-align: left; padding: 6px 9px; border-bottom: 1px solid var(--line); vertical-align: top; }
th { font-size: 12px; color: var(--muted); white-space: nowrap; cursor: pointer; user-select: none; }
th.sorted { color: var(--text); }
td.title { overflow-wrap: anywhere; min-width: 22em; }
td.num { text-align: right; }
td.muted { color: var(--muted); white-space: nowrap; max-width: 16em; overflow: hidden; text-overflow: ellipsis; }
.tree ul { list-style: none; margin: 0; padding-left: 20px; border-left: 1px solid var(--line); }
.tree > ul { padding-left: 0; border-left: 0; }
.tree details > summary { cursor: pointer; padding: 2px 0; }
.tree .leaf { padding: 2px 0 2px 14px; }
.tree .line { display: inline-flex; gap: 6px; align-items: baseline; flex-wrap: wrap; }
.tree .line.done .t { color: var(--muted); text-decoration: line-through; }
.tools { display: flex; gap: 8px; margin-bottom: 10px; }
.attention h2 { font-size: 15px; margin: 18px 0 2px; }
.attention .row { display: flex; gap: 8px; align-items: baseline; flex-wrap: wrap; padding: 6px 0; border-bottom: 1px solid var(--line); }
.attention .why { color: var(--muted); font-size: 12px; }
.backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, .35); }
aside.drawer { position: fixed; top: 0; right: 0; bottom: 0; width: min(760px, 100%); overflow: auto; background: var(--panel); border-left: 1px solid var(--line); padding: 16px 20px 40px; box-shadow: -6px 0 24px rgba(0, 0, 0, .25); }
aside.drawer h2 { margin: 6px 0 8px; font-size: 18px; overflow-wrap: anywhere; }
aside.drawer .head { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
aside.drawer .close { margin-left: auto; }
aside.drawer h3 { margin: 18px 0 4px; font-size: 12px; text-transform: uppercase; letter-spacing: .04em; color: var(--muted); }
dl.times { display: grid; grid-template-columns: max-content 1fr; gap: 2px 14px; margin: 8px 0; }
dl.times dt { color: var(--muted); } dl.times dd { margin: 0; overflow-wrap: anywhere; }
.text { white-space: pre-wrap; overflow-wrap: anywhere; margin: 0; font: inherit; }
code, .path { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 12px; overflow-wrap: anywhere; }
aside.drawer ul { margin: 0; padding-left: 20px; }
.fold > summary { cursor: pointer; color: var(--muted); }
.fold[open] > summary { margin-bottom: 4px; }
.note { margin: 8px 0; padding: 7px 10px; background: var(--note); border-radius: 6px; border-left: 3px solid var(--accent); }
.note.change { background: var(--change); border-left-color: var(--muted); font-size: 13px; }
.note .who { color: var(--muted); font-size: 12px; margin-bottom: 3px; }
.chain { color: var(--muted); }
`;
