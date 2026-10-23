/**
 * The backlog page's script (`html.ts` places it inline after the data). It reads the JSON block and builds every node
 * with `createElement` and text nodes: nothing from the data is parsed as HTML. It uses no template literals and no
 * backticks, so it can live in a template string here without escapes.
 */

export const PAGE_JS = String.raw`
'use strict';
(function () {
  var data = JSON.parse(document.getElementById('data').textContent);
  var items = data.items;
  var byRef = {};
  items.forEach(function (it) { byRef[it.ref] = it; });
  var BOARD = ['ongoing', 'todo', 'backlog', 'done'];
  var TITLES = { ongoing: 'Ongoing', todo: 'Todo', backlog: 'Backlog (triage)', done: 'Done' };
  var PRIORITIES = ['p0', 'p1', 'p2', 'p3'];
  var FOLD_CHARS = 700;
  var FOLD_LINES = 14;
  var view = { name: 'board', params: {} };
  var saved = {};

  // ---- building blocks: createElement and text nodes only ----
  function append(el, child) {
    if (child === undefined || child === null || child === false) return;
    if (Array.isArray(child)) { child.forEach(function (c) { append(el, c); }); return; }
    el.appendChild(typeof child === 'string' ? document.createTextNode(child) : child);
  }
  function h(tag, attrs) {
    var el = document.createElement(tag);
    Object.keys(attrs || {}).forEach(function (key) {
      var value = attrs[key];
      if (value === undefined || value === null || value === false) return;
      if (key === 'class') el.className = value;
      else if (key === 'text') el.textContent = value;
      else if (key === 'value') el.value = value;
      else if (key.slice(0, 2) === 'on') el.addEventListener(key.slice(2), value);
      else el.setAttribute(key, value === true ? '' : value);
    });
    for (var i = 2; i < arguments.length; i++) append(el, arguments[i]);
    return el;
  }
  function clear(el) { while (el.firstChild) el.removeChild(el.firstChild); }

  // ---- times: shown as of the snapshot ----
  function pad(n) { return n < 10 ? '0' + n : String(n); }
  function stamp(t) {
    var d = new Date(t);
    return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()) + ' ' + pad(d.getHours()) + ':' + pad(d.getMinutes());
  }
  function span(ms) {
    var s = Math.floor(Math.max(0, ms) / 1000);
    if (s < 60) return s + 's';
    var m = Math.floor(s / 60);
    if (m < 60) return m + 'm';
    var hr = Math.floor(m / 60);
    if (hr < 24) return hr + 'h' + (m % 60 ? (m % 60) + 'm' : '');
    var d = Math.floor(hr / 24);
    return d + 'd' + (hr % 24 ? (hr % 24) + 'h' : '');
  }
  function ago(t) { return span(data.generatedAt - t) + ' ago'; }
  function cycle(it) { return it.startedAt && it.doneAt && it.doneAt >= it.startedAt ? it.doneAt - it.startedAt : undefined; }

  // ---- the hash holds the view, its filters and the open item ----
  function dec(text) { try { return decodeURIComponent(text); } catch (e) { return text; } }
  function parseHash() {
    var raw = location.hash.replace(/^#/, '');
    var q = raw.indexOf('?');
    var params = {};
    if (q >= 0) raw.slice(q + 1).split('&').forEach(function (pair) {
      if (!pair) return;
      var i = pair.indexOf('=');
      params[dec(i < 0 ? pair : pair.slice(0, i))] = i < 0 ? '' : dec(pair.slice(i + 1));
    });
    return { name: q < 0 ? raw : raw.slice(0, q), params: params };
  }
  function hashFor(name, params) {
    var parts = Object.keys(params || {}).filter(function (k) { return params[k]; }).map(function (k) { return encodeURIComponent(k) + '=' + encodeURIComponent(params[k]); });
    return '#' + name + (parts.length ? '?' + parts.join('&') : '');
  }
  function setParams(name, params) {
    view = { name: name, params: params };
    saved[name] = params;
    try { history.replaceState(null, '', hashFor(name, params)); } catch (e) { /* a browser that refuses: the page still works, the hash just does not follow */ }
  }

  // ---- pieces shared by the views ----
  function link(it) { return h('a', { href: '#' + it.ref, text: it.ref, class: 'ref' }); }
  function stateChip(it) { return h('span', { class: 'chip ' + it.state, text: it.state }); }
  function prioChip(it) { return h('span', { class: 'chip ' + it.priority, text: it.priority }); }
  function stale(it) { return it.state === 'ongoing' && !it.assigneeLive; }
  function staleChip(it) { return stale(it) ? h('span', { class: 'chip stale', title: it.assignee ? it.assignee + ' is not a running session' : 'ongoing without an assignee', text: 'stale claim' }) : null; }
  function idle(it) { return it.state !== 'done' && data.generatedAt - it.updatedAt > data.idleMs; }
  function lineCount(text) { return text.split('\n').length; }

  /** A text block; long text folds into <details> and stays whole. */
  function longText(text, label) {
    var body = h('pre', { class: 'text', text: text });
    if (text.length <= FOLD_CHARS && lineCount(text) <= FOLD_LINES) return body;
    return h('details', { class: 'fold' }, h('summary', { text: label + ': ' + lineCount(text) + ' lines, ' + text.length + ' characters' }), body);
  }

  // ---- header ----
  function median(values) {
    var sorted = values.slice().sort(function (a, b) { return a - b; });
    var mid = Math.floor(sorted.length / 2);
    return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
  }
  function stat(label, value) { return h('span', { class: 'stat' }, label + ' ', h('b', { text: String(value) })); }
  function header() {
    var counts = {};
    var prios = {};
    items.forEach(function (it) { counts[it.state] = (counts[it.state] || 0) + 1; prios[it.priority] = (prios[it.priority] || 0) + 1; });
    var cycles = items.map(cycle).filter(function (c) { return c !== undefined; });
    var stats = h('div', { class: 'stats' }, stat('items', items.length));
    BOARD.forEach(function (state) { stats.appendChild(stat(state, counts[state] || 0)); });
    PRIORITIES.forEach(function (p) { stats.appendChild(stat(p, prios[p] || 0)); });
    stats.appendChild(stat('median cycle (started to done)', cycles.length ? span(median(cycles)) + ' over ' + cycles.length : 'none yet'));
    var top = document.getElementById('top');
    clear(top);
    top.appendChild(h('h1', { text: 'Backlog: ' + data.repo }));
    top.appendChild(h('div', { class: 'snapshot', text: 'Snapshot generated ' + stamp(data.generatedAt) + ' (' + span(Date.now() - data.generatedAt) + ' ago). Not live: times below are as of the snapshot. Run /backlog html to refresh it.' }));
    top.appendChild(stats);
    top.appendChild(h('nav', { class: 'tabs', id: 'tabs' }));
    document.title = 'Backlog: ' + data.repo;
  }
  var attentionCount = items.filter(function (it) { return stale(it) || idle(it); }).length;
  var VIEWS = { board: 'Board', table: 'Table', tree: 'Tree', attention: 'Attention' };
  function tabs() {
    var nav = document.getElementById('tabs');
    clear(nav);
    Object.keys(VIEWS).forEach(function (name) {
      nav.appendChild(h('a', { href: hashFor(name, saved[name] || {}), class: name === view.name ? 'active' : '' }, VIEWS[name], name === 'attention' && attentionCount ? h('span', { class: 'badge', text: String(attentionCount) }) : null));
    });
  }

  // ---- board ----
  function card(it) {
    var meta = h('div', { class: 'meta' });
    if (it.assignee) meta.appendChild(h('span', { title: 'assignee', text: it.assignee }));
    if (it.tags.length) meta.appendChild(h('span', { text: it.tags.map(function (t) { return '#' + t; }).join(' ') }));
    meta.appendChild(h('span', { title: 'last update, as of the snapshot', text: ago(it.updatedAt) }));
    return h('a', { class: 'card ' + it.state, href: '#' + it.ref },
      h('div', { class: 'top' }, h('span', { class: 'ref', text: it.ref }), prioChip(it), staleChip(it), it.derived.length ? h('span', { class: 'chip', title: 'items derived from this one', text: it.derived.length + ' derived' }) : null),
      h('div', { class: 'title', text: it.title }), meta);
  }
  function viewBoard(params) {
    var board = h('div', { class: 'board' });
    BOARD.forEach(function (state) {
      var group = items.filter(function (it) { return it.state === state; });
      var collapsed = state === 'done' && params.done !== '1';
      var head = h('h2', null, TITLES[state] + ' (' + group.length + ')');
      if (state === 'done' && group.length) {
        head.appendChild(h('button', { type: 'button', text: collapsed ? 'Show' : 'Hide', onclick: function () {
          setParams('board', collapsed ? { done: '1' } : {});
          render();
        } }));
      }
      var column = h('section', { class: 'column' }, head);
      if (!collapsed) {
        group.forEach(function (it) { column.appendChild(card(it)); });
        if (!group.length) column.appendChild(h('div', { class: 'empty', text: 'None' }));
      }
      board.appendChild(column);
    });
    return board;
  }

  // ---- table ----
  function haystack(it) {
    if (!it.hay) {
      it.hay = [it.ref, it.title, it.goal, it.body, it.why, it.plan, it.state, it.priority, it.assignee || '', it.createdBy, it.paths.join(' '), it.evidence.join(' '), it.tags.join(' ')]
        .concat(it.notes.map(function (n) { return n.author + ' ' + n.text; })).join('\n').toLowerCase();
    }
    return it.hay;
  }
  function seq(it) { return Number(it.ref.slice(1)); }
  var COLUMNS = [
    { key: 'ref', label: 'Ref', value: seq, first: 'asc' },
    { key: 'state', label: 'State', value: function (it) { return BOARD.indexOf(it.state); }, first: 'asc' },
    { key: 'priority', label: 'Pri', value: function (it) { return PRIORITIES.indexOf(it.priority); }, first: 'asc' },
    { key: 'title', label: 'Title', value: function (it) { return it.title.toLowerCase(); }, first: 'asc' },
    { key: 'assignee', label: 'Assignee', value: function (it) { return (it.assignee || '').toLowerCase(); }, first: 'asc' },
    { key: 'tags', label: 'Tags', value: function (it) { return it.tags.join(' '); }, first: 'asc' },
    { key: 'updated', label: 'Updated', value: function (it) { return it.updatedAt; }, first: 'desc' },
    { key: 'created', label: 'Created', value: function (it) { return it.createdAt; }, first: 'desc' },
    { key: 'notes', label: 'Notes', value: function (it) { return it.notes.length; }, first: 'desc' },
    { key: 'derived', label: 'Derived', value: function (it) { return it.derived.length; }, first: 'desc' }
  ];
  function distinct(pick) {
    var seen = {};
    items.forEach(function (it) { pick(it).forEach(function (v) { seen[v] = true; }); });
    return Object.keys(seen).sort();
  }
  function select(label, options, current, onchange) {
    var el = h('select', { 'aria-label': label, onchange: function () { onchange(el.value); } }, h('option', { value: '', text: label + ': all' }));
    options.forEach(function (o) { var opt = h('option', { value: o, text: o }); opt.selected = o === current; el.appendChild(opt); });
    return el;
  }
  function viewTable(params) {
    var f = { state: params.state || '', p: params.p || '', tag: params.tag || '', assignee: params.assignee || '', q: params.q || '', sort: params.sort || 'updated', dir: params.dir === 'asc' ? 'asc' : 'desc' };
    var body = h('tbody');
    var count = h('span', { class: 'snapshot' });
    var headRow = h('tr');
    function sync() {
      var plain = f.sort === 'updated' && f.dir === 'desc';
      setParams('table', { state: f.state, p: f.p, tag: f.tag, assignee: f.assignee, q: f.q, sort: plain ? '' : f.sort, dir: plain ? '' : f.dir });
      rows();
    }
    function rows() {
      var needle = f.q.trim().toLowerCase();
      var column = COLUMNS.filter(function (c) { return c.key === f.sort; })[0] || COLUMNS[6];
      var shown = items.filter(function (it) {
        return (!f.state || it.state === f.state) && (!f.p || it.priority === f.p) && (!f.tag || it.tags.indexOf(f.tag) >= 0) && (!f.assignee || it.assignee === f.assignee) && (!needle || haystack(it).indexOf(needle) >= 0);
      }).sort(function (a, b) {
        var x = column.value(a), y = column.value(b);
        var order = x < y ? -1 : x > y ? 1 : 0;
        return (f.dir === 'asc' ? order : -order) || seq(a) - seq(b);
      });
      count.textContent = shown.length + ' of ' + items.length + ' items';
      clear(body);
      shown.forEach(function (it) {
        body.appendChild(h('tr', null,
          h('td', null, link(it)), h('td', null, stateChip(it), ' ', staleChip(it)), h('td', null, prioChip(it)),
          h('td', { class: 'title' }, h('a', { href: '#' + it.ref, text: it.title })),
          h('td', { class: 'muted', title: it.assignee || '', text: it.assignee || '' }), h('td', { class: 'muted', title: it.tags.join(' '), text: it.tags.map(function (t) { return '#' + t; }).join(' ') }),
          h('td', { class: 'muted', title: stamp(it.updatedAt), text: ago(it.updatedAt) }), h('td', { class: 'muted', title: stamp(it.createdAt), text: ago(it.createdAt) }),
          h('td', { class: 'num', text: String(it.notes.length) }), h('td', { class: 'num', text: String(it.derived.length) })));
      });
      if (!shown.length) body.appendChild(h('tr', null, h('td', { colspan: String(COLUMNS.length), class: 'empty', text: 'No item matches.' })));
      clear(headRow);
      COLUMNS.forEach(function (c) {
        var mark = c.key === f.sort ? (f.dir === 'asc' ? ' \u25b2' : ' \u25bc') : '';
        headRow.appendChild(h('th', { class: c.key === f.sort ? 'sorted' : '', title: 'Sort by ' + c.label.toLowerCase(), text: c.label + mark, onclick: function () {
          f.dir = f.sort === c.key ? (f.dir === 'asc' ? 'desc' : 'asc') : c.first;
          f.sort = c.key;
          sync();
        } }));
      });
    }
    var search = h('input', { type: 'search', placeholder: 'Search every field and note', 'aria-label': 'Search', value: f.q, oninput: function () { f.q = search.value; sync(); } });
    var filters = h('div', { class: 'filters' }, search,
      select('State', BOARD, f.state, function (v) { f.state = v; sync(); }),
      select('Priority', PRIORITIES, f.p, function (v) { f.p = v; sync(); }),
      select('Tag', distinct(function (it) { return it.tags; }), f.tag, function (v) { f.tag = v; sync(); }),
      select('Assignee', distinct(function (it) { return it.assignee ? [it.assignee] : []; }), f.assignee, function (v) { f.assignee = v; sync(); }),
      count);
    rows();
    return h('div', null, filters, h('table', null, h('thead', null, headRow), body));
  }

  // ---- tree: derivation by from links ----
  function viewTree() {
    var roots = items.filter(function (it) { return !it.from; });
    var linked = roots.filter(function (it) { return it.derived.length; });
    var alone = roots.filter(function (it) { return !it.derived.length; });
    var folds = [];
    function line(it) { return h('span', { class: 'line ' + it.state }, link(it), stateChip(it), prioChip(it), h('span', { class: 't', text: it.title })); }
    function node(it, depth, seen) {
      var kids = it.derived.filter(function (ref) { return byRef[ref] && !seen[ref]; });
      if (!kids.length) return h('li', { class: 'leaf' }, line(it));
      var inner = {};
      Object.keys(seen).forEach(function (k) { inner[k] = true; });
      inner[it.ref] = true;
      var details = h('details', null, h('summary', null, line(it), ' ', h('span', { class: 'chip', text: kids.length + ' derived' })),
        h('ul', null, kids.map(function (ref) { return node(byRef[ref], depth + 1, inner); })));
      details.open = depth < 2;
      folds.push(details);
      return h('li', null, details);
    }
    var tree = h('div', { class: 'tree' }, h('ul', null, linked.map(function (it) { return node(it, 0, {}); })));
    if (!linked.length) tree.appendChild(h('div', { class: 'empty', text: 'No item has a derived item yet.' }));
    if (alone.length) tree.appendChild(h('details', { class: 'fold' }, h('summary', { text: 'Items without a link (' + alone.length + ')' }), h('ul', null, alone.map(function (it) { return h('li', { class: 'leaf' }, line(it)); }))));
    var tools = h('div', { class: 'tools' },
      h('button', { type: 'button', text: 'Expand all', onclick: function () { folds.forEach(function (d) { d.open = true; }); } }),
      h('button', { type: 'button', text: 'Collapse all', onclick: function () { folds.forEach(function (d) { d.open = false; }); } }));
    return h('div', null, tools, tree);
  }

  // ---- attention ----
  function viewAttention() {
    var claims = items.filter(stale);
    var idleItems = items.filter(idle).sort(function (a, b) { return a.updatedAt - b.updatedAt; });
    function row(it, why) { return h('div', { class: 'row' }, link(it), stateChip(it), prioChip(it), h('a', { href: '#' + it.ref, text: it.title }), h('span', { class: 'why', text: why })); }
    var root = h('div', { class: 'attention' });
    root.appendChild(h('h2', { text: 'Stale claims (' + claims.length + ')' }));
    root.appendChild(h('div', { class: 'why', text: 'Ongoing items whose assignee was not a running session when the snapshot was made.' }));
    claims.forEach(function (it) { root.appendChild(row(it, (it.assignee ? it.assignee + ' is not running' : 'no assignee') + (it.startedAt ? '; claimed ' + ago(it.startedAt) : ''))); });
    if (!claims.length) root.appendChild(h('div', { class: 'empty', text: 'None.' }));
    root.appendChild(h('h2', { text: 'Not updated in ' + span(data.idleMs) + ' (' + idleItems.length + ')' }));
    root.appendChild(h('div', { class: 'why', text: 'Open items with no change or note since then.' }));
    idleItems.forEach(function (it) { root.appendChild(row(it, 'last update ' + ago(it.updatedAt))); });
    if (!idleItems.length) root.appendChild(h('div', { class: 'empty', text: 'None.' }));
    return root;
  }

  // ---- item detail ----
  function section(title, content) { return content ? [h('h3', { text: title }), content] : null; }
  function chain(it) {
    var refs = [];
    var seen = {};
    for (var at = it.from; at && byRef[at] && !seen[at]; at = byRef[at].from) { seen[at] = true; refs.unshift(at); }
    if (!refs.length) return null;
    var box = h('div', { class: 'chain' }, 'Origin: ');
    refs.forEach(function (ref, i) {
      if (i) box.appendChild(document.createTextNode(' \u203a '));
      box.appendChild(h('a', { href: '#' + ref, title: byRef[ref].title, text: ref }));
    });
    box.appendChild(document.createTextNode(' \u203a ' + it.ref));
    return box;
  }
  function times(it) {
    var dl = h('dl', { class: 'times' });
    function add(label, value) { if (value) dl.appendChild(h('dt', { text: label })); if (value) dl.appendChild(h('dd', { text: value })); }
    add('Created', stamp(it.createdAt) + ' (' + ago(it.createdAt) + ') by ' + it.createdBy);
    if (it.startedAt) add('Claimed', stamp(it.startedAt) + ' (' + ago(it.startedAt) + ')');
    if (it.doneAt) add('Done', stamp(it.doneAt) + ' (' + ago(it.doneAt) + ')' + (cycle(it) !== undefined ? ', took ' + span(cycle(it)) : ''));
    add('Updated', stamp(it.updatedAt) + ' (' + ago(it.updatedAt) + ')');
    if (it.assignee) add('Assignee', it.assignee + (it.state === 'ongoing' ? (it.assigneeLive ? ' (running)' : ' (not running)') : ''));
    if (it.sourceSession) add('Source session', it.sourceSession);
    add('Version', String(it.version));
    return dl;
  }
  function list(entries, tag) { return entries.length ? h('ul', null, entries.map(function (e) { return h('li', null, h(tag || 'span', { text: e })); })) : null; }
  function derivedList(it) {
    if (!it.derived.length) return null;
    return h('ul', null, it.derived.map(function (ref) {
      var d = byRef[ref];
      return h('li', null, link(d), ' ', stateChip(d), ' ', h('a', { href: '#' + ref, text: d.title }));
    }));
  }
  function noteBox(note, i) {
    var who = stamp(note.at) + ' (' + ago(note.at) + ') \u00b7 ' + note.author + (note.kind === 'change' ? ' \u00b7 change' : '');
    return h('div', { class: 'note ' + note.kind }, h('div', { class: 'who', text: '#' + (i + 1) + ' ' + who }), longText(note.text, note.kind === 'change' ? 'change' : 'note'));
  }
  function detail(it) {
    return h('div', null,
      h('div', { class: 'head' }, h('span', { class: 'ref', text: it.ref }), stateChip(it), prioChip(it), staleChip(it), h('a', { class: 'close', href: hashFor(view.name, view.params), text: 'Close' })),
      h('h2', { text: it.title }),
      it.tags.length ? h('div', null, it.tags.map(function (t) { return h('span', { class: 'chip', text: '#' + t }); }).reduce(function (all, chip) { return all.concat([chip, ' ']); }, [])) : null,
      times(it), chain(it),
      section('Goal', it.goal ? longText(it.goal, 'goal') : null),
      section('Why', it.why ? longText(it.why, 'why') : null),
      section('Plan', it.plan ? longText(it.plan, 'plan') : null),
      section('Where', list(it.paths, 'code')),
      section('Evidence', list(it.evidence)),
      section('Body', it.body ? longText(it.body, 'body') : null),
      section('Derived items (' + it.derived.length + ')', derivedList(it)),
      section('Notes and changes (' + it.notes.length + ')', it.notes.length ? h('div', null, it.notes.map(noteBox)) : null));
  }

  // ---- routing ----
  var main = document.getElementById('main');
  var drawer = document.getElementById('drawer');
  var backdrop = document.getElementById('backdrop');
  function render() {
    clear(main);
    var build = { board: viewBoard, table: viewTable, tree: viewTree, attention: viewAttention }[view.name];
    main.appendChild(build(view.params));
    tabs();
  }
  function showDetail(ref) {
    var it = byRef[ref];
    clear(drawer);
    drawer.appendChild(it ? detail(it) : h('div', null, h('p', { text: 'No item ' + ref + ' in this snapshot.' }), h('a', { href: hashFor(view.name, view.params), text: 'Close' })));
    drawer.hidden = false;
    backdrop.hidden = false;
    drawer.scrollTop = 0;
    document.title = ref + (it ? ': ' + it.title : '') + ' \u2013 ' + data.repo;
  }
  function route() {
    var r = parseHash();
    if (/^[Bb][0-9]+$/.test(r.name)) {
      if (!main.firstChild) render();
      showDetail(r.name.toUpperCase());
      return;
    }
    drawer.hidden = true;
    backdrop.hidden = true;
    document.title = 'Backlog: ' + data.repo;
    view = VIEWS[r.name] ? { name: r.name, params: r.params } : { name: 'board', params: {} };
    saved[view.name] = view.params;
    render();
  }
  backdrop.addEventListener('click', function () { location.hash = hashFor(view.name, view.params); });
  document.addEventListener('keydown', function (e) { if (e.key === 'Escape' && !drawer.hidden) location.hash = hashFor(view.name, view.params); });
  window.addEventListener('hashchange', route);
  header();
  route();
})();
`;
