// Page template: one self-contained HTML file. The page script inlines the Markdown engine and the
// client views via Function#toString, so each view function stays self-contained.
import { mdKit } from './markdown.mjs';
import { client } from './client.mjs';
import { documentView } from './client-document.mjs';
import { widgetView } from './client-widgets.mjs';
import { diagramView } from './client-diagrams.mjs';
import { searchView } from './client-search.mjs';

const PANZOOM_URL = 'https://cdn.jsdelivr.net/npm/@panzoom/panzoom@4.6.2/dist/panzoom.min.js';
const MERMAID_URL = 'https://cdn.jsdelivr.net/npm/mermaid@12.1.0/dist/mermaid.esm.min.mjs';

// Template: one file, pinned renderer libraries load only for the content that needs them.
// Design: violet accents, quiet reading surfaces, consistent type and spacing.
const CSS = String.raw`
:root{--bg:#f8f6fc;--panel:#fff;--soft:#f0ebf8;--line:#ded6ed;--ink:#21182f;--muted:#655a75;--accent:#7135b7;--accent-ink:#57248f;--accent-soft:#eee3fc;--gradient:linear-gradient(120deg,#7135b7,#a32788);--glow:#ede0fc;--on-accent:#fff;--mark:#ffe58a;--code:#f5f1fa;
--red:#b42318;--red-bg:#fde8e6;--amber:#93590a;--amber-bg:#fdf0d9;--blue:#1d5bbf;--blue-bg:#e3ecfb;--green:#1b7442;--green-bg:#e0f3e7;--gray:#4d5763;--gray-bg:#eceff2;
--serif:"Iowan Old Style","Charter","Bitstream Charter","Source Serif Pro","Sitka Text",Cambria,Georgia,serif;--sans:system-ui,-apple-system,"Segoe UI",Roboto,"Helvetica Neue",sans-serif;--mono:ui-monospace,"SF Mono",SFMono-Regular,Menlo,Consolas,monospace;--nav:264px;--rail:272px}
:root[data-theme=dark]{--bg:#100d18;--panel:#1c1628;--soft:#271e36;--line:#3e3152;--ink:#eee8f7;--muted:#b6a8c9;--accent:#c5a0ff;--accent-ink:#dec4ff;--accent-soft:#352249;--gradient:linear-gradient(120deg,#b38aff,#eb9bdb);--glow:#2b1944;--on-accent:#100d18;--mark:#6a5410;--code:#211a2f;
--red:#ff8a7f;--red-bg:#3d1f1d;--amber:#f0b45a;--amber-bg:#3a2c15;--blue:#8fb4ff;--blue-bg:#1d2a44;--green:#6fd29a;--green-bg:#173325;--gray:#b3bcc6;--gray-bg:#26303a}
*{box-sizing:border-box}html{scroll-padding-top:72px}
body{margin:0;background:radial-gradient(ellipse 80% 380px at 30% 0,var(--glow),transparent),var(--bg);color:var(--ink);font:15px/1.55 var(--sans);-webkit-font-smoothing:antialiased}
a{color:var(--accent);text-underline-offset:2px}a.missing{color:var(--muted);text-decoration:underline dotted;cursor:not-allowed}button,select,input{font:inherit;color:inherit}
:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
kbd{font:12px var(--mono);border:1px solid var(--line);border-bottom-width:2px;border-radius:4px;padding:0 5px;background:var(--panel)}
.muted{color:var(--muted);font-size:13px;margin:6px 4px}
header.top{position:sticky;top:0;z-index:30;display:flex;align-items:center;gap:10px;height:56px;padding:0 16px;background:var(--panel);border-bottom:1px solid var(--line);box-shadow:0 2px 16px #7135b70a}
header.top::before{content:"";position:absolute;inset:0 0 auto;height:3px;background:var(--gradient)}
#site-title{font-weight:650;font-size:15px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;cursor:pointer;min-width:0;background:none;border:0;padding:4px 2px;text-align:left}
.mark-logo{width:26px;height:26px;flex:none;color:var(--accent)}
.grow{flex:1}
.search{position:relative;width:min(400px,38vw)}
.search input{width:100%;height:34px;padding:0 34px 0 12px;border:1px solid var(--line);border-radius:6px;background:var(--soft);outline:none}
.search input:focus{background:var(--panel);border-color:var(--accent)}
.search kbd{position:absolute;right:8px;top:7px}
#results{position:absolute;top:40px;left:0;right:0;max-height:64vh;overflow:auto;background:var(--panel);border:1px solid var(--line);border-radius:8px;box-shadow:0 12px 32px rgba(10,20,30,.16);display:none;padding:4px}
#results.open{display:block}.res-count{margin:4px 8px;font-size:12px;color:var(--muted)}
.result{display:block;padding:7px 9px;border-radius:5px;color:var(--ink);cursor:pointer;text-decoration:none;font-size:13.5px}
.result small{display:block;color:var(--muted);font-size:12px}.result.active,.result:hover{background:var(--accent-soft)}
.empty{padding:12px;color:var(--muted);font-size:13px}
mark{background:var(--mark);color:inherit;border-radius:2px}
.tool{height:34px;min-width:34px;padding:0 9px;display:inline-grid;place-items:center;border:1px solid var(--line);border-radius:6px;background:var(--panel);cursor:pointer}
.tool:hover{border-color:var(--accent);color:var(--accent)}.tool svg{width:17px;height:17px}
select.tool{padding:0 6px;font-size:13px}
#menu{display:none}
#progress{position:fixed;top:56px;left:0;right:0;height:2px;background:var(--gradient);transform:scaleX(0);transform-origin:0 0;z-index:31}
.layout{display:grid;grid-template-columns:var(--nav) minmax(0,1fr) var(--rail);max-width:1560px;margin:0 auto}
body.single .layout{grid-template-columns:minmax(0,1fr) var(--rail);max-width:1240px}
body.single nav.side{display:none}
nav.side,aside.rail{position:sticky;top:56px;height:calc(100vh - 56px);overflow:auto;padding:16px 14px 32px}
nav.side{border-right:1px solid var(--line)}
#navfilter{width:100%;height:30px;padding:0 9px;border:1px solid var(--line);border-radius:5px;background:var(--panel);font-size:13px;margin-bottom:10px}
.nav-home{display:block;padding:6px 8px;border-radius:5px;color:var(--ink);text-decoration:none;font-weight:600;font-size:13.5px}
.nav-group h3{font-size:12px;font-weight:600;color:var(--muted);margin:16px 8px 4px}.nav-group h3::first-letter,.lib-group h2::first-letter{text-transform:uppercase}
#nav a{display:block;padding:6px 8px;border-radius:5px;color:var(--ink);text-decoration:none;font-size:13.5px;line-height:1.35}
#nav a small{display:block;color:var(--muted);font-size:11.5px}
#nav a:hover{background:var(--soft)}#nav a.active{background:var(--accent-soft);color:var(--accent-ink);font-weight:600}
main{min-width:0;padding:36px 44px 96px}
#library-summary{max-width:760px;margin:0 auto 24px;padding:16px 20px;display:flex;flex-wrap:wrap;align-items:baseline;gap:10px 18px;font-size:12px;color:var(--muted);border:1px solid var(--line);border-radius:12px;background:var(--panel);box-shadow:inset 3px 0 var(--accent)}
#library-summary:empty{display:none}
.kind{color:var(--accent-ink);font-weight:650}
.stats{display:inline-flex;flex-wrap:wrap;gap:6px 12px;width:100%;padding-top:10px;border-top:1px solid var(--line)}
article{max-width:760px;margin:0 auto;font:16px/1.7 var(--sans);overflow-wrap:break-word}article p,article li{max-width:75ch}article a{overflow-wrap:anywhere}article h1,article h2{ text-wrap:balance }
article h1,article h2,article h3,article h4{font-family:var(--sans);line-height:1.25;letter-spacing:-.01em;position:relative}
article h1{font-size:clamp(28px,3vw,38px);font-weight:750;margin:6px 0 16px;letter-spacing:-.035em;padding-bottom:18px}article h1::after{content:"";position:absolute;bottom:0;left:0;width:64px;height:4px;border-radius:4px;background:var(--gradient)}
article h2{font-size:23px;font-weight:650;margin:36px 0 14px;padding-top:24px;border-top:1px solid var(--line)}
article h3{font-size:18.5px;font-weight:650;margin:32px 0 8px}
article h4{font-size:16px;font-weight:650;margin:24px 0 6px}
article p.lead{font-size:18px;line-height:1.6;color:var(--ink);margin-bottom:28px}
.anchor{margin-left:8px;color:var(--muted);text-decoration:none;font-weight:400;opacity:0}
h1:hover .anchor,h2:hover .anchor,h3:hover .anchor,h4:hover .anchor,.anchor:focus{opacity:1}
.flash{animation:flash 1.4s ease-out}@keyframes flash{0%{background:var(--mark)}100%{background:transparent}}
article ul,article ol{padding-left:1.4em}article li{margin:4px 0}article li>ul,article li>ol{margin:4px 0}
article li.task{list-style:none;margin-left:-1.3em}article li.task input{appearance:none;-webkit-appearance:none;width:15px;height:15px;margin:0 8px 0 0;vertical-align:-2px;border:1.5px solid var(--muted);border-radius:4px;background:var(--panel);display:inline-grid;place-content:center}article li.task input:checked{background:var(--accent);border-color:var(--accent)}article li.task input:checked::after{content:'';width:4px;height:8px;border:solid var(--on-accent);border-width:0 2px 2px 0;transform:translateY(-1px) rotate(45deg)}
article code{font:.84em var(--mono);background:var(--code);border:1px solid var(--line);border-radius:4px;padding:.08em .35em}
article a.coderef{text-decoration:none}article a.coderef code{color:var(--accent-ink);border-color:color-mix(in srgb,var(--accent) 40%,var(--line));background:var(--accent-soft)}
article a.coderef:hover code{border-color:var(--accent)}
article a.missing{text-decoration:underline dotted;color:var(--muted)}
article a.ext::after{content:"↗";font-size:.75em;margin-left:2px;color:var(--muted)}
article pre{position:relative;margin:18px 0;background:var(--code);border:1px solid var(--line);border-radius:8px;overflow:auto;font:13.5px/1.6 var(--mono)}
article pre code{display:block;padding:12px 16px;background:none;border:0;font-size:inherit}
.codebar{position:sticky;left:0;display:flex;justify-content:space-between;align-items:center;padding:4px 8px 4px 16px;border-bottom:1px solid var(--line);font:12px var(--sans);color:var(--muted)}
.codebar button{border:0;background:none;color:var(--muted);cursor:pointer;font-size:12px;padding:2px 6px;border-radius:4px}.codebar button:hover{color:var(--accent);background:var(--panel)}
.tk-k{color:#8a3ffc}.tk-s{color:#107c41}.tk-c{color:var(--muted);font-style:italic}.tk-n{color:#b45309}.tk-t{color:#0e6aa8}.tk-add{color:var(--green);background:var(--green-bg)}.tk-del{color:var(--red);background:var(--red-bg)}.tk-m{color:var(--blue)}
:root[data-theme=dark] .tk-k{color:#c4a1ff}:root[data-theme=dark] .tk-s{color:#7fd3a0}:root[data-theme=dark] .tk-n{color:#f5b971}:root[data-theme=dark] .tk-t{color:#7cc4f2}
.tablewrap{margin:20px 0;overflow:auto;border:1px solid var(--line);border-radius:8px;background:var(--panel)}
table{border-collapse:collapse;width:100%;font:14px/1.5 var(--sans)}
th,td{padding:9px 12px;text-align:left;vertical-align:top;border-bottom:1px solid var(--line)}
th{background:var(--soft);font-weight:600;font-size:13px;position:sticky;top:0}tr:last-child td{border-bottom:0}
tbody tr:hover td{background:color-mix(in srgb,var(--accent-soft) 50%,transparent)}
.chip{display:inline-block;font:600 12px/1.6 var(--sans);padding:0 7px;border-radius:3px;white-space:nowrap;background:var(--gray-bg);color:var(--gray)}
.t-red{background:var(--red-bg);color:var(--red)}.t-amber{background:var(--amber-bg);color:var(--amber)}.t-blue{background:var(--blue-bg);color:var(--blue)}.t-green{background:var(--green-bg);color:var(--green)}
blockquote{margin:20px 0;padding:2px 18px;border-left:3px solid var(--line);color:var(--muted)}
.alert{margin:20px 0;padding:10px 16px;border-left:4px solid var(--blue);background:var(--blue-bg);border-radius:0 6px 6px 0;font-size:16px}
.alert p{margin:6px 0}.alert-title{font:650 13.5px var(--sans);color:var(--blue)}
.alert-tip{border-color:var(--green);background:var(--green-bg)}.alert-tip .alert-title{color:var(--green)}
.alert-important{border-color:var(--accent);background:var(--accent-soft)}.alert-important .alert-title{color:var(--accent-ink)}
.alert-warning{border-color:var(--amber);background:var(--amber-bg)}.alert-warning .alert-title{color:var(--amber)}
.alert-caution{border-color:var(--red);background:var(--red-bg)}.alert-caution .alert-title{color:var(--red)}
hr{border:0;border-top:1px solid var(--line);margin:36px 0}img{max-width:100%;height:auto;border-radius:6px}.image-error{padding:12px;border:1px solid var(--line);color:var(--ink);background:var(--soft)}
details{margin:16px 0;padding:8px 14px;border:1px solid var(--line);border-radius:6px;background:var(--panel)}summary{cursor:pointer;font:600 15px var(--sans)}
.fnref a{text-decoration:none;font:600 .7em var(--sans)}
.footnotes{margin-top:48px;padding-top:12px;border-top:1px solid var(--line);font-size:14.5px;color:var(--muted)}.fnback{text-decoration:none}
.data-widget{margin:24px 0;border:1px solid var(--line);border-radius:12px;overflow:hidden;background:var(--panel)}
.data-widget figcaption{padding:14px 18px;background:linear-gradient(110deg,var(--accent-soft),var(--soft));font-weight:650}.widget-actions{display:flex;flex-wrap:wrap;gap:8px;padding:12px 16px}.widget-actions button,.widget-actions select,.widget-actions input{border:1px solid var(--line);border-radius:6px;background:var(--panel);padding:6px 10px;min-height:36px;max-width:100%;min-width:0}.widget-actions input{flex:1;min-width:120px}.widget-actions button{cursor:pointer}.widget-status,.widget-detail{padding:0 16px;font-size:13px;color:var(--muted)}.widget-detail:not(:empty){padding:12px 16px;border-top:1px solid var(--line)}.widget-view{height:380px;width:100%;touch-action:pan-y}.widget-large .widget-view{height:70vh;min-height:480px}.data-widget details{padding:12px 16px;border-top:1px solid var(--line)}.data-widget summary{cursor:pointer;color:var(--accent)}.widget-data{overflow:auto}.data-widget pre{max-height:320px;overflow:auto}.data-widget [hidden]{display:none!important}
@media print{.widget-actions,.widget-status,.data-widget details pre{display:none!important}.data-widget details{display:block}.data-widget details::details-content{content-visibility:visible}.widget-view{break-inside:avoid}}
.diagram{margin:24px 0;box-shadow:0 8px 24px #7135b708;border:1px solid var(--line);border-radius:10px;background:var(--panel);overflow:hidden}
.diagram figcaption{display:flex;align-items:center;gap:6px;padding:6px 10px;border-bottom:1px solid var(--line);background:linear-gradient(110deg,var(--accent-soft),var(--soft));font:12px var(--sans);color:var(--muted)}
.diagram figcaption b{color:var(--ink);font-weight:600}.diagram figcaption span{margin-right:auto;text-transform:capitalize}
.diagram figcaption button{border:1px solid var(--line);background:var(--panel);border-radius:4px;padding:2px 8px;font-size:12px;cursor:pointer;color:var(--muted)}
.diagram figcaption button:disabled{opacity:.55;cursor:wait}.diagram figcaption button:hover{color:var(--accent);border-color:var(--accent)}
.diagram-find{display:flex;gap:12px;align-items:center;padding:8px 12px;border-bottom:1px solid var(--line)}.diagram-find input{min-width:0;flex:1;border:1px solid var(--line);border-radius:6px;padding:6px 10px;background:var(--panel)}.diagram-find output{font-size:12px;color:var(--muted)}.diagram-match{outline:3px solid var(--accent);outline-offset:3px;filter:drop-shadow(0 0 3px var(--accent))}
.diagram .canvas{display:flex;justify-content:safe center;padding:18px;overflow:auto;font:13px var(--sans);color:var(--muted)}
.diagram .canvas svg{max-width:100%;height:auto}.diagram pre{margin:0;border:0;border-radius:0}
.diagram [hidden]{display:none!important}
.diagram .note{margin:0;padding:6px 12px;font:12.5px var(--sans);color:var(--amber);border-top:1px solid var(--line)}
.lib-filters{display:flex;flex-wrap:wrap;gap:6px;margin:4px 0 18px;font-family:var(--sans)}
.fchip{border:1px solid var(--line);background:var(--panel);color:var(--ink);border-radius:999px;padding:4px 11px;font-size:13px;cursor:pointer}
.fchip small,.tfilter small{color:var(--muted);margin-left:3px}
.fchip:hover{border-color:var(--accent)}.fchip.on{background:var(--accent);border-color:var(--accent);color:var(--on-accent)}.fchip.on small{color:inherit;opacity:.8}
.toc-filters{display:flex;flex-direction:column}.tfilter{background:none;border:0;text-align:left;cursor:pointer;font:inherit;font-size:13.5px;color:var(--muted);padding:4px 10px}.tfilter:hover{color:var(--ink)}.tfilter.on{color:var(--accent);font-weight:600}
.lib-kind{font-size:11.5px;font-weight:600;color:var(--accent);background:var(--accent-soft);border-radius:3px;padding:2px 7px;white-space:nowrap;flex:none}
.lib-head{margin-bottom:12px}.lib-group h2 small{font:400 13px var(--mono);color:var(--muted);margin-left:8px}
.lib-list{list-style:none;padding:0;margin:0}
.lib-list li{margin:0;border-bottom:1px solid var(--line)}
.lib-list a{display:block;padding:14px 4px;color:var(--ink);text-decoration:none}
.lib-list a:hover .lib-title{color:var(--accent)}
.lib-row{display:flex;align-items:baseline;gap:10px;font-family:var(--sans)}
.lib-title{font-weight:650;font-size:17px}.lib-date{margin-left:auto;font-size:13px;color:var(--muted);white-space:nowrap}
.lib-lead{display:block;margin-top:4px;font-size:15.5px;color:var(--muted);line-height:1.55}
.lib-path{display:block;margin-top:4px;font:12px var(--mono);color:var(--muted)}
.tabs{display:flex;gap:4px;margin-bottom:10px;border-bottom:1px solid var(--line)}
.tabs button{border:0;background:none;padding:6px 8px;font-size:13px;font-weight:600;color:var(--muted);cursor:pointer;border-bottom:2px solid transparent;margin-bottom:-1px}
.tabs button[aria-selected=true]{color:var(--ink);border-bottom-color:var(--accent)}
#toc a{display:block;padding:4px 8px;border-left:2px solid transparent;color:var(--muted);text-decoration:none;font-size:13px;line-height:1.4}
#toc a.l3{padding-left:20px;font-size:12.5px}#toc a:hover{color:var(--ink)}#toc a.active{color:var(--accent-ink);border-left-color:var(--accent);font-weight:600}
.linkbtn{border:0;background:none;color:var(--accent);cursor:pointer;font-size:12.5px;padding:2px 0;margin-bottom:6px}
.src-group h3{font-size:12px;font-weight:600;color:var(--muted);margin:14px 0 4px;display:flex;justify-content:space-between}
.src{display:block;padding:5px 6px;border-radius:4px;text-decoration:none;color:var(--ink);font-size:12.5px;line-height:1.35;word-break:break-word}
.src:hover{background:var(--soft)}.src-url{display:block;color:var(--muted);font:11px var(--mono)}.src-count{float:right;color:var(--muted);font-size:11px;margin-left:6px}
#toast{position:fixed;bottom:22px;left:50%;transform:translate(-50%,16px);opacity:0;background:var(--ink);color:var(--bg);padding:8px 14px;border-radius:6px;font-size:13px;transition:.18s;pointer-events:none;z-index:60}
#toast.show{opacity:1;transform:translate(-50%,0)}
dialog{border:1px solid var(--line);border-radius:12px;background:var(--panel);color:var(--ink);padding:16px;max-width:94vw;max-height:94vh}
dialog::backdrop{background:rgba(8,12,18,.65)}#modal-close{display:block;margin:0 0 12px auto;padding:8px 16px;border:1px solid var(--line);border-radius:6px;background:var(--soft);color:var(--ink);cursor:pointer}
#render-check{width:min(560px,94vw);padding:24px;box-shadow:0 24px 80px #0004}#render-check h2{margin:0 0 16px;font-size:22px}#render-check p{line-height:1.6}#render-check p+p{color:var(--muted);font-size:13px}#render-check-close{border:1px solid var(--line);border-radius:6px;padding:8px 16px;background:var(--accent-soft);color:var(--accent-ink);cursor:pointer}
#modal{width:min(1200px,94vw)}.diagram-toolbar{display:flex;align-items:center;gap:12px;flex-wrap:wrap}#diagram-tools{display:flex;gap:6px}#diagram-tools button{min-height:36px;min-width:36px;border:1px solid var(--line);border-radius:6px;background:var(--soft);cursor:pointer}#diagram-tools button:disabled{opacity:.5;cursor:wait}#diagram-scale{font:13px var(--mono);color:var(--muted)}#modal-close{margin:0 0 0 auto}#diagram-help{font-size:12px;color:var(--muted)}#modalbox{height:65vh;max-width:100%;overflow:auto;border:1px solid var(--line);border-radius:8px;background:var(--panel)}#modalbox.interactive{display:flex;align-items:center;justify-content:center;overflow:hidden}.diagram-stage{flex-shrink:0}#modalbox svg{display:block}
#mobile-toc{display:none;max-width:760px;margin:16px auto 24px}#mobile-toc nav{display:grid;gap:8px;padding:12px 0 4px}#mobile-toc .l3{padding-left:16px;font-size:14px}
.skip{position:fixed;left:12px;top:-100px;z-index:100;padding:12px 20px;background:var(--panel);color:var(--ink);border:2px solid var(--accent);border-radius:6px}.skip:focus{top:12px}
@media (max-width:1180px){.layout{grid-template-columns:var(--nav) minmax(0,1fr)}body.single .layout{grid-template-columns:minmax(0,1fr)}aside.rail{display:none}#mobile-toc:not([hidden]){display:block}}
@media (max-width:820px){.layout,body.single .layout{grid-template-columns:minmax(0,1fr)}#menu{display:inline-grid}nav.side{position:fixed;top:56px;left:0;bottom:0;width:min(320px,86vw);background:var(--panel);z-index:40;transform:translateX(-102%);transition:transform .2s;height:auto}
body.menu nav.side{transform:none;box-shadow:0 0 40px rgba(0,0,0,.25)}main{padding:20px 18px 80px}.search{width:auto;flex:1}#site-title,.mark-logo,#editor{display:none}#library-summary{padding:12px 14px;font-size:12px;gap:8px 12px}article{font-size:16px}article h1{font-size:28px}.stats{margin-left:0}}
@media (prefers-reduced-motion:reduce){*{animation:none!important;transition:none!important;scroll-behavior:auto!important}}
@media (max-width:600px){.diagram figcaption{flex-wrap:wrap}.diagram figcaption span{flex:1;min-width:calc(100% - 95px)}.diagram figcaption button{min-height:36px}.diagram .canvas{justify-content:flex-start}.diagram .canvas svg{min-width:0;max-width:100%;flex-shrink:1}}
@media print{.diagram-find,.widget-actions,header.top,nav.side,aside.rail,#mobile-toc,#library-summary,#progress,.codebar,.anchor,.diagram figcaption button{display:none!important}.layout,body.single .layout{display:block}main{padding:0}article{max-width:none;font-size:11.5pt}body{background:#fff;color:#000}
article a.ext::after{content:" (" attr(href) ")";font-size:.8em;color:#555}article pre,.diagram,.tablewrap{break-inside:avoid}article pre code{white-space:pre-wrap}}
`;

const ICONS = {
  logo: '<svg class="mark-logo" viewBox="0 0 32 32" aria-hidden="true"><path fill="currentColor" d="M16 3c-6 0-10 4.3-10 10 0 3.2 1.4 5.6 3.4 7.2-.9 2.3-2.9 3.9-5.4 4.4 2.9 1.5 6 .9 8-1.1.5 2.2 2.1 4.1 4 5.5 1.9-1.4 3.5-3.3 4-5.5 2 2 5.1 2.6 8 1.1-2.5-.5-4.5-2.1-5.4-4.4 2-1.6 3.4-4 3.4-7.2C26 7.3 22 3 16 3Zm-4 9.5a1.7 1.7 0 1 1 0 3.4 1.7 1.7 0 0 1 0-3.4Zm8 0a1.7 1.7 0 1 1 0 3.4 1.7 1.7 0 0 1 0-3.4Z"/></svg>',
  menu: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M4 6h16M4 12h16M4 18h16"/></svg>',
  theme: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z"/></svg>',
  print: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M6 9V3h12v6M6 18H4a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2M6 14h12v7H6z"/></svg>',
};

export function pageHtml(payload, title) {
  const json = JSON.stringify(payload).replace(/</g, '\\u003c').replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029');
  const views = `{ documentView: ${documentView.toString()}, widgetView: ${widgetView.toString()}, diagramView: ${diagramView.toString()}, searchView: ${searchView.toString()} }`;
  const script = `(${client.toString()})((${mdKit.toString()})(), ${JSON.stringify(MERMAID_URL)}, ${JSON.stringify(PANZOOM_URL)}, ${views});`.replace(/<\/(script)/gi, '<\\/$1');
  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="generator" content="octocode render-report">
<title>${esc(title)}</title>
<style>${CSS}</style>
</head>
<body>
<a class="skip" href="#page">Skip to content</a>
<header class="top">
  <button class="tool" id="menu" type="button" aria-label="Documents">${ICONS.menu}</button>
  ${ICONS.logo}
  <button id="site-title" type="button" title="Back to start">${esc(title)}</button>
  <span class="grow"></span>
  <div class="search" role="search"><input id="q" type="search" placeholder="Search" autocomplete="off" aria-label="Search the report"><kbd>/</kbd><div id="results" role="listbox"></div></div>
  <select class="tool" id="editor" aria-label="Open code references in" title="Open code references in"><option value="vscode">VS Code</option><option value="cursor">Cursor</option><option value="windsurf">Windsurf</option><option value="file">File</option></select>
  <button class="tool" id="theme" type="button" aria-label="Toggle theme" title="Theme (t)">${ICONS.theme}</button>
  <button class="tool" id="verify-render" type="button" aria-label="Render check" title="Check diagrams and widgets">✓</button>
  <button class="tool" id="print" type="button" aria-label="Print" title="Print or save as PDF">${ICONS.print}</button>
</header>
<div id="progress"></div>
<div class="layout">
  <nav class="side" aria-label="Documents"><input id="navfilter" type="search" placeholder="Filter documents" aria-label="Filter documents"><div id="nav"></div></nav>
  <main><div id="library-summary"></div><details id="mobile-toc" hidden></details><article id="page" tabindex="-1"></article></main>
  <aside class="rail" aria-label="Contents and sources">
    <div class="tabs" role="tablist"><button type="button" role="tab" data-tab="toc" id="tab-toc" aria-selected="true">Contents</button><button type="button" role="tab" data-tab="sources" id="tab-sources" aria-selected="false">Sources</button></div>
    <div class="rail-panel" id="toc"></div>
    <div class="rail-panel" id="sources" hidden></div>
  </aside>
</div>
<dialog id="render-check" aria-label="Render check"><h2>Render check</h2><p id="render-check-output" role="status"></p><p>This checks diagram, widget and image loading, not factual accuracy or accessibility conformance.</p><button id="render-check-close" autofocus>Close check</button></dialog>
<dialog id="modal" aria-label="Expanded diagram"><div class="diagram-toolbar"><div id="diagram-tools" role="group" aria-label="Diagram zoom"><button data-zoom="out" aria-label="Zoom out" disabled>−</button><button data-zoom="in" aria-label="Zoom in" disabled>+</button><button data-zoom="fit" disabled>Fit</button><button data-zoom="actual" disabled>100%</button></div><output id="diagram-scale" aria-live="polite"></output><button id="modal-close" type="button" autofocus>Close diagram</button></div><p id="diagram-help">Drag to pan · scroll or pinch to zoom · + / − to zoom · 0 to fit · arrows to pan · Escape to close</p><div id="modalbox" tabindex="0" role="region" aria-label="Interactive expanded diagram" aria-describedby="diagram-help"></div></dialog>
<div id="toast" role="status" aria-live="polite"></div>
<script type="application/json" id="report-data">${json}</script>
<script>${script}</script>
</body>
</html>
`;
}
