// Page view: Mermaid diagrams, their in-diagram search, and the expanded pan/zoom dialog.
// Inlined into the page via Function#toString: it may use only ctx and browser globals.
// oxlint-disable-next-line max-lines-per-function -- serialized whole via Function#toString
export function diagramView(ctx) {
  const { kit, $, $$, esc, MERMAID_URL, PANZOOM_URL } = ctx;
  const DIAGRAMS = { flowchart: 'Flowchart', graph: 'Flowchart', sequencediagram: 'Sequence', classdiagram: 'Class diagram', statediagram: 'State diagram', 'statediagram-v2': 'State diagram', erdiagram: 'Entity relationship', gantt: 'Timeline plan', pie: 'Pie chart', mindmap: 'Mind map', timeline: 'Timeline', 'xychart-beta': 'Chart', xychart: 'Chart', quadrantchart: 'Quadrant', journey: 'User journey', gitgraph: 'Git graph', sankey: 'Sankey', 'sankey-beta': 'Sankey', block: 'Block diagram', 'block-beta': 'Block diagram', architecture: 'Architecture', 'architecture-beta': 'Architecture', kanban: 'Kanban', packet: 'Packet', 'packet-beta': 'Packet', requirementdiagram: 'Requirements', c4context: 'C4 context', c4container: 'C4 containers', c4component: 'C4 components', c4dynamic: 'C4 interactions', c4deployment: 'C4 deployment', 'agentflow-beta': 'Agent flow', 'swimlane-beta': 'Swimlanes', eventmodeling: 'Event modeling', 'radar-beta': 'Radar chart', treemap: 'Treemap', 'venn-beta': 'Venn diagram', 'wardley-beta': 'Wardley map', 'cynefin-beta': 'Cynefin framework', 'treeview-beta': 'Tree view', ishikawa: 'Cause and effect', 'ishikawa-beta': 'Cause and effect', 'usecase-beta': 'Use cases', 'railroad-beta': 'Railroad diagram', 'railroad-ebnf-beta': 'EBNF grammar', 'railroad-abnf-beta': 'ABNF grammar', 'railroad-peg-beta': 'PEG grammar', 'flowchart-elk': 'Flowchart · ELK', info: 'Mermaid version' };
  const diagramLabel = (src) => { const w = (kit.frontmatter(src).body.replace(/^%%.*$/gm, '').trim().split(/\s/)[0] || '').toLowerCase(); return DIAGRAMS[w] || w.replace(/-beta$/, '').replace(/-/g, ' '); };

  // ----- Diagrams -----
  let mermaidReady = null;
  const getMermaid = () => (mermaidReady ||= new Promise(resolve => { const timer = setTimeout(()=>resolve(null),10000); import(MERMAID_URL).then(m=>{clearTimeout(timer);resolve(m.default);},()=>{clearTimeout(timer);resolve(null);}); }));
  async function renderDiagrams(root) {
    const figs = $$('.diagram', root);
    if (!figs.length) return;
    const mermaid = await getMermaid();
    const dark = document.documentElement.dataset.theme === 'dark';
    if (mermaid) {
      const css = getComputedStyle(document.documentElement), v = (n) => css.getPropertyValue(n).trim();
      mermaid.initialize({ startOnLoad: false, suppressErrorRendering: true, securityLevel: 'strict', theme: 'base', fontFamily: 'system-ui, sans-serif', xyChart: { height: 340 }, themeVariables: {
        xyChart: { plotColorPalette: [v('--accent'), v('--muted'), '#c2410c', '#15803d'].join(','), backgroundColor: v('--panel'), titleColor: v('--ink'), xAxisLabelColor: v('--muted'), yAxisLabelColor: v('--muted'), xAxisTitleColor: v('--muted'), yAxisTitleColor: v('--muted'), xAxisLineColor: v('--line'), yAxisLineColor: v('--line'), xAxisTickColor: v('--line'), yAxisTickColor: v('--line') },
        pie1: '#7135b7', pie2: '#c2410c', pie3: '#15803d', pie4: '#59636f', pieStrokeColor: v('--panel'), pieTitleTextColor: v('--ink'), pieSectionTextColor: '#fff', pieLegendTextColor: v('--ink'),
        darkMode: dark, background: v('--panel'), fontSize: '16px',
        primaryColor: v('--accent-soft'), primaryBorderColor: v('--accent'), primaryTextColor: v('--ink'),
        secondaryColor: v('--soft'), secondaryTextColor: v('--ink'), tertiaryColor: v('--panel'), tertiaryTextColor: v('--ink'), edgeLabelBackground: v('--panel'), lineColor: v('--muted'), textColor: v('--ink'),
        clusterBkg: v('--soft'), clusterBorder: v('--line'), noteBkgColor: v('--soft'), noteBorderColor: v('--line'),
        actorBkg: v('--accent-soft'), actorBorder: v('--accent'), actorTextColor: v('--ink'), signalColor: v('--ink'), signalTextColor: v('--ink'),
      } });
    }
    for (const [k, fig] of figs.entries()) {
      if (!fig.isConnected) return;
      const canvas = $('.canvas', fig);
      const showSource = (note) => { fig.dataset.render = 'failed'; canvas.hidden = true; $('pre', fig).hidden = false; $('[data-act="src"]', fig).setAttribute('aria-pressed', 'true'); fig.insertAdjacentHTML('beforeend', `<p class="note">${esc(note)}</p>`); };
      if (!mermaid) { showSource('Mermaid could not load (offline?). Showing the diagram source.'); continue; }
      try {
        const { svg } = await mermaid.render(`mmd-${Date.now()}-${k}`, fig.dataset.src);
        if (!fig.isConnected) return;
        canvas.innerHTML = svg; fig.dataset.render = 'ready';
        const finder = document.createElement('div'); finder.className = 'diagram-find';
        finder.innerHTML = '<input type="search" aria-label="Find text in diagram" placeholder="Find in diagram"><output aria-live="polite"></output>';
        canvas.before(finder);
        $('input',finder).addEventListener('input', event => {
          const term = event.target.value.trim().toLowerCase();
          $$('.diagram-match',canvas).forEach(el=>el.classList.remove('diagram-match'));
          const labels = $$('text,foreignObject',canvas).filter(el=>!el.parentElement.closest('text,foreignObject') && el.textContent.toLowerCase().includes(term));
          if(term) labels.forEach(el=>el.classList.add('diagram-match'));
          $('output',finder).textContent = term ? `${labels.length} matches` : '';
        });
        canvas.hidden = !$('pre', fig).hidden;
        $('[data-act="zoom"]', fig).disabled = false;
        $('[data-act="download"]', fig).disabled = false;
        const title = $('svg title', canvas)?.textContent;
        if (title) canvas.setAttribute('aria-label', title);
        $('[data-act="src"]', fig).textContent = canvas.hidden ? 'Show diagram' : 'Show source';
      } catch (e) { if (fig.isConnected) showSource(`Diagram error: ${String(e?.message || e).split('\n')[0]}`); }
    }
  }

  // ----- Expanded diagram -----
  const modal = $('#modal');
  let modalTrigger = null;
  let expandedDiagram = null;
  let panzoomReady;
  const getPanzoom = () => (panzoomReady ||= new Promise((resolve) => {
    const script = document.createElement('script');
    script.src = PANZOOM_URL;
    const timer = setTimeout(() => resolve(null), 8000);
    script.addEventListener('load', () => { clearTimeout(timer); resolve(window.Panzoom || null); });
    script.addEventListener('error', () => { clearTimeout(timer); resolve(null); });
    document.head.append(script);
  }));
  const fitDiagram = () => {
    const d = expandedDiagram;
    if (!d?.panzoom) return;
    const box = $('#modalbox');
    d.panzoom.zoom(Math.min((box.clientWidth - 32) / d.width, (box.clientHeight - 32) / d.height, 1), { animate: false });
    d.panzoom.pan(0, 0, { animate: false });
  };
  const openDiagram = async (svg, trigger) => {
    const box = $('#modalbox'), stage = document.createElement('div');
    const view = svg.viewBox.baseVal;
    const d = { svg, canvas: svg.parentElement, style: svg.getAttribute('style'), width: view.width || 800, height: view.height || 600 };
    expandedDiagram = d;
    stage.className = 'diagram-stage';
    stage.style.width = d.width + 'px'; stage.style.height = d.height + 'px';
    svg.style.cssText += ';width:100%;height:100%;max-width:none;min-width:0';
    stage.append(svg); box.replaceChildren(stage);
    box.classList.remove('interactive');
    box.removeAttribute('style');
    $('#diagram-help').textContent = 'Loading zoom controls. The diagram remains scrollable.';
    $$('#diagram-tools button').forEach(b => b.disabled = true);
    $('#diagram-scale').textContent = 'Loading controls…';
    modalTrigger = trigger; modal.showModal();
    const Panzoom = await getPanzoom();
    if (expandedDiagram !== d) return;
    if (!Panzoom) { $('#diagram-scale').textContent = 'Zoom unavailable'; $('#diagram-help').textContent = 'Scroll to explore the diagram. Escape closes this view.'; return; }
    box.classList.add('interactive');
    $('#diagram-help').textContent = 'Drag to pan · scroll or pinch to zoom · + / − to zoom · 0 to fit · arrows to pan · Escape to close';
    d.panzoom = Panzoom(stage, { minScale: 0.02, maxScale: 8, step: 0.2, animate: false });
    d.wheel = (event) => { event.preventDefault(); d.panzoom.zoomWithWheel(event); };
    box.addEventListener('wheel', d.wheel, { passive: false });
    stage.addEventListener('panzoomchange', () => { $('#diagram-scale').textContent = Math.round(d.panzoom.getScale() * 100) + '%'; });
    d.resize = new ResizeObserver(fitDiagram); d.resize.observe(box);
    $$('#diagram-tools button').forEach(b => b.disabled = false);
    fitDiagram();
    box.focus();
  };
  const closeModal = () => {
    if (expandedDiagram) {
      const d = expandedDiagram;
      d.resize?.disconnect(); d.panzoom?.destroy();
      if (d.wheel) $('#modalbox').removeEventListener('wheel', d.wheel);
      if (d.style === null) d.svg.removeAttribute('style'); else d.svg.setAttribute('style', d.style);
      d.canvas.append(d.svg);
      $('#modalbox').replaceChildren();
      $('#modalbox').removeAttribute('style');
      expandedDiagram = null;
    }
    modal.close();
    modalTrigger?.focus();
    modalTrigger = null;
  };
  $('#modal-close').addEventListener('click', closeModal);
  $('#diagram-tools').addEventListener('click', (event) => {
    const action = event.target.closest('button')?.dataset.zoom;
    const p = expandedDiagram?.panzoom;
    if (!p) return;
    if (action === 'in') p.zoomIn();
    if (action === 'out') p.zoomOut();
    if (action === 'fit') fitDiagram();
    if (action === 'actual') { p.zoom(1); p.pan(0, 0); }
  });
  modal.addEventListener('click', (event) => { if (event.target === modal) closeModal(); });
  modal.addEventListener('cancel', (event) => { event.preventDefault(); closeModal(); });

  return { diagramLabel, renderDiagrams, openDiagram, closeModal, fitDiagram, modal, expanded: () => expandedDiagram };
}
