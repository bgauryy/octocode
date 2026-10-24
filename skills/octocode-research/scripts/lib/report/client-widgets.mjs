// Page view: interactive graph (Cytoscape.js) and chart (ECharts) widgets with data tables.
// Inlined into the page via Function#toString: it may use only ctx and browser globals.
// oxlint-disable-next-line max-lines-per-function -- serialized whole via Function#toString
export function widgetView(ctx) {
  const { kit, $, $$, esc, slugify, copy } = ctx;
  const widgetDisposers = new Set();
  function disposeWidgets() { for (const dispose of widgetDisposers) dispose(); widgetDisposers.clear(); }
  const widgetLibraries = new Map();
  function loadWidgetLibrary(kind) {
    if (widgetLibraries.has(kind)) return widgetLibraries.get(kind);
    const config = kind === 'graph'
      ? ['https://cdn.jsdelivr.net/npm/cytoscape@3.33.1/dist/cytoscape.min.js', 'cytoscape']
      : ['https://cdn.jsdelivr.net/npm/echarts@6.0.0/dist/echarts.min.js', 'echarts'];
    const promise = new Promise((resolve, reject) => {
      if (window[config[1]]) { resolve(window[config[1]]); return; }
      const script = document.createElement('script');
      const listeners = new AbortController();
      const timer = setTimeout(() => done(new Error('Renderer loading timed out')), 10000);
      const done = (error) => { clearTimeout(timer); listeners.abort(); if (error) { script.remove(); widgetLibraries.delete(kind); reject(error); } else resolve(window[config[1]]); };
      script.addEventListener('load', () => window[config[1]] ? done() : done(new Error('Renderer unavailable')), { signal: listeners.signal });
      script.addEventListener('error', () => done(new Error('Renderer could not load; check your connection')), { signal: listeners.signal });
      script.src = config[0]; document.head.append(script);
    });
    widgetLibraries.set(kind, promise);
    return promise;
  }
  async function renderWidgets(root) {
    await Promise.all($$('.widget-src', root).map(async pre => {
      const kind = pre.dataset.widget, source = $('code', pre).textContent;
      const card = document.createElement('figure'); card.className = 'data-widget'; card.dataset.render = 'pending'; card.id = pre.id;
      card.innerHTML = '<figcaption></figcaption><div class="widget-actions"></div><p class="widget-status" role="status"></p><div class="widget-view" role="img"></div><div class="widget-detail" aria-live="polite"></div><details class="widget-values"><summary>Show data</summary><div class="widget-data"></div></details><details class="widget-source"><summary>Show source</summary><pre><code></code></pre></details>';
      $('pre code', card).textContent = source;
      pre.replaceWith(card);
      const status = $('.widget-status', card), view = $('.widget-view', card), actions = $('.widget-actions', card);
      let d;
      try { d = kit.widget(kind, source); } catch(e) { card.dataset.render = 'failed'; $('figcaption', card).textContent = 'Invalid ' + kind; status.textContent = e.message; $('.widget-source', card).open = true; view.hidden = true; return; }
      $('figcaption', card).textContent = d.title; view.setAttribute('aria-label', d.title + '. Use Show data for a text alternative.');
      const table = kind === 'graph'
        ? '<h3>Nodes</h3><table><thead><tr><th>Node</th><th>Description</th><th>Evidence</th></tr></thead><tbody>' + d.nodes.map(n => `<tr><td>${esc(n.label)}</td><td>${esc(n.description)}</td><td>${n.url ? `<a href="${esc(n.url)}" target="_blank" rel="noopener noreferrer">Source</a>` : '—'}</td></tr>`).join('') + '</tbody></table><h3>Connections</h3><table><thead><tr><th>From</th><th>To</th><th>Relationship</th></tr></thead><tbody>' + d.edges.map(e => `<tr><td>${esc(d.nodes.find(n => n.id === e.source).label)}</td><td>${esc(d.nodes.find(n => n.id === e.target).label)}</td><td>${esc(e.label)}</td></tr>`).join('') + '</tbody></table>'
        : '<table><thead><tr><th>Category</th>' + d.series.map(s => `<th>${esc(s.name)}${d.unit ? ' (' + esc(d.unit) + ')' : ''}</th>`).join('') + '</tr></thead><tbody>' + d.labels.map((label,i) => `<tr><td>${esc(label)}</td>${d.series.map(s => `<td>${s.values[i]}</td>`).join('')}</tr>`).join('') + '</tbody></table>';
      $('.widget-data',card).innerHTML = table;
      const button = (label, fn) => { const b = document.createElement('button'); b.type = 'button'; b.textContent = label; b.addEventListener('click', fn); actions.append(b); return b; };
      button('Copy source', () => copy(source, 'Widget data copied'));
      status.textContent = 'Loading interactive ' + kind + '…';
      let instance, observer, disposed = false;
      const cleanup = () => { disposed = true; observer?.disconnect(); if (kind === 'graph') instance?.destroy(); else instance?.dispose(); };
      widgetDisposers.add(cleanup);
      try {
        const library = await loadWidgetLibrary(kind);
        if (disposed || !card.isConnected) return;
        const css = getComputedStyle(document.documentElement), color = n => css.getPropertyValue(n).trim();
        if (kind === 'graph') {
          const search = document.createElement('input'); search.type = 'search'; search.placeholder = 'Find nodes'; search.setAttribute('aria-label','Find nodes in ' + d.title); actions.prepend(search);
          const select = document.createElement('select'); select.setAttribute('aria-label','Select node in ' + d.title); select.innerHTML = '<option value="">All nodes</option>' + d.nodes.map(n => `<option value="${esc(n.id)}">${esc(n.label)}</option>`).join(''); actions.append(select);
          instance = library({ container: view, elements: [...d.nodes.map(n => ({data:n})), ...d.edges.map(e => ({data:e}))], layout:{name:'breadthfirst',directed:true,padding:32,spacingFactor:1.4}, minZoom:.15,maxZoom:3,userZoomingEnabled:false,style:[
            {selector:'node',style:{'label':'data(label)','background-color':color('--accent'),'color':color('--ink'),'font-size':14,'text-valign':'bottom','text-margin-y':8,'text-wrap':'wrap','text-max-width':140,'width':28,'height':28}},
            {selector:'edge',style:{'label':'data(label)','font-size':11,'color':color('--muted'),'line-color':color('--muted'),'target-arrow-color':color('--muted'),'target-arrow-shape':'triangle','curve-style':'bezier','width':1.5,'text-background-color':color('--panel'),'text-background-opacity':1,'text-background-padding':3}},
            {selector:'.muted',style:{opacity:.15}}, {selector:'.focused',style:{'border-width':4,'border-color':color('--ink')}}
          ]});
          const cy = instance;
          const focus = id => {
            cy.elements().removeClass('muted focused');
            const detail = $('.widget-detail',card); detail.replaceChildren();
            if (!id) { cy.fit(undefined,32); status.textContent = `${d.nodes.length} nodes · ${d.edges.length} connections`; return; }
            const n = cy.getElementById(id), data = n.data();
            cy.elements().not(n.closedNeighborhood()).addClass('muted'); n.addClass('focused'); cy.fit(n.closedNeighborhood(),60);
            const strong = document.createElement('strong'); strong.textContent = data.label; detail.append(strong, document.createTextNode(' — ' + (data.description || 'No description supplied.')));
            if (data.url) { const a = document.createElement('a'); a.href = data.url; a.target='_blank'; a.rel='noopener noreferrer'; a.textContent='Open source'; detail.append(' ',a); }
            status.textContent = 'Showing ' + data.label + ' and its neighbors';
          };
          select.addEventListener('change', () => focus(select.value));
          cy.on('tap','node',event => { select.value=event.target.id(); focus(select.value); });
          search.addEventListener('input', () => {
            const term=search.value.trim().toLowerCase();
            const matches=cy.nodes().filter(n=>n.data('label').toLowerCase().includes(term));
            cy.elements().removeClass('muted focused'); select.value=''; $('.widget-detail',card).replaceChildren();
            if(term) { cy.elements().not(matches).addClass('muted'); matches.addClass('focused'); if(matches.length) cy.fit(matches,60); } else cy.fit(undefined,32);
            status.textContent=term ? `${matches.length} matching nodes` : `${d.nodes.length} nodes · ${d.edges.length} connections`;
          });
          button('−',()=>cy.zoom({level:Math.max(cy.minZoom(),cy.zoom()/1.25),renderedPosition:{x:cy.width()/2,y:cy.height()/2}})).setAttribute('aria-label','Zoom out');
          button('+',()=>cy.zoom({level:Math.min(cy.maxZoom(),cy.zoom()*1.25),renderedPosition:{x:cy.width()/2,y:cy.height()/2}})).setAttribute('aria-label','Zoom in');
          button('Reset',()=>{search.value='';select.value='';focus('');});
          status.textContent = `${d.nodes.length} nodes · ${d.edges.length} connections · select a node to explore`;
        } else {
          instance=library.init(view,null,{renderer:'svg'});
          instance.setOption({animation:false,color:[color('--accent'),'#d97706','#15803d','#2563eb'],textStyle:{color:color('--ink'),fontFamily:'system-ui'},aria:{enabled:true,label:{description:d.title + '. ' + d.series.map(s=>s.name + ': ' + d.labels.map((label,i)=>label + ' ' + s.values[i] + (d.unit?' '+d.unit:'')).join(', ')).join('. ')},decal:{show:true}},tooltip:{trigger:d.type==='pie'?'item':'axis',renderMode:'richText'},legend:{bottom:0,type:'scroll',textStyle:{color:color('--ink')}},grid:{left:20,right:20,top:24,bottom:64,containLabel:true},...(d.type==='pie'?{}:{xAxis:{type:'category',data:d.labels,axisLabel:{color:color('--muted'),hideOverlap:true}},yAxis:{type:'value',name:d.unit,axisLabel:{color:color('--muted')},splitLine:{lineStyle:{color:color('--line')}}}}),series:d.series.map(s=>({name:s.name,type:d.type,...(d.type==='pie'?{radius:['35%','65%'],label:{color:color('--ink')},data:d.labels.map((name,i)=>({name,value:s.values[i]}))}:{data:s.values})}))});
          status.textContent = 'Use the legend to compare series; open Show data for exact values.';
        }
        button(kind==='graph'?'Export PNG':'Export SVG',()=>{const a=document.createElement('a');a.download=slugify(d.title)+(kind==='graph'?'.png':'.svg');a.href=kind==='graph'?instance.png({full:true,bg:color('--panel'),scale:2}):instance.getDataURL({type:'svg',backgroundColor:color('--panel')});a.click();});
        const expand=button('Expand',()=>{card.classList.toggle('widget-large');expand.textContent=card.classList.contains('widget-large')?'Collapse':'Expand';expand.setAttribute('aria-pressed',String(card.classList.contains('widget-large')));});
        expand.setAttribute('aria-pressed','false');
        observer=new ResizeObserver(()=>{if(kind==='graph'){instance.resize();}else instance.resize();}); observer.observe(view); card.dataset.render = 'ready';
      } catch(e) { if(!disposed){card.dataset.render = 'failed'; status.textContent=e.message + '. Data and source remain available.'; $$('details',card).forEach(el=>{el.open=true;});view.hidden=true;} }
    }));
  }

  return { disposeWidgets, renderWidgets };
}
