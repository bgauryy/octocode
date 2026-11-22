// Generated from packages/octocode-graph/src by scripts/build.mjs.
class RelationIndex {
  outgoing = /* @__PURE__ */ new Map();
  endpoints = /* @__PURE__ */ new Set();
  kinds;
  constructor(kinds, relations) {
    this.kinds = new Set(kinds);
    for (const edge of relations) {
      if (!this.kinds.has(edge.relation))
        throw new Error(`Undeclared relation ${edge.relation}`);
      this.endpoints.add(edge.source);
      this.endpoints.add(edge.target);
      const neighbors = this.outgoing.get(edge.source) ?? /* @__PURE__ */ new Map();
      const edges = neighbors.get(edge.relation) ?? [];
      edges.push(edge);
      neighbors.set(edge.relation, edges);
      this.outgoing.set(edge.source, neighbors);
    }
  }
  has(node) {
    return this.endpoints.has(node);
  }
  from(node, kind) {
    return this.outgoing.get(node)?.get(kind) ?? [];
  }
}
class PatternError extends Error {
}
function compile(expression, supported) {
  const tokens = expression.match(/[A-Za-z_][A-Za-z_0-9]*|[^\s]/gu) ?? [];
  const states = [];
  const state = () => states.push([]) - 1;
  const link = (a, b) => states[a].push({ to: b });
  const fragments = [];
  const operators = [];
  const precedence = (op) => op === "\xB7" ? 2 : op === "|" ? 1 : 0;
  function combine() {
    const op = operators.pop();
    const right = fragments.pop();
    const left = fragments.pop();
    if (op === "\xB7") {
      link(left.end, right.start);
      fragments.push({ start: left.start, end: right.end });
    } else {
      const start2 = state(), end2 = state();
      link(start2, left.start);
      link(start2, right.start);
      link(left.end, end2);
      link(right.end, end2);
      fragments.push({ start: start2, end: end2 });
    }
  }
  function binary(op) {
    while (operators.length && precedence(operators.at(-1)) >= precedence(op))
      combine();
    operators.push(op);
  }
  let expectingAtom = true;
  let quantified = false;
  for (const token of tokens) {
    if (token === "*" || token === "+") {
      if (expectingAtom || quantified)
        throw new PatternError(`Unexpected token ${token}`);
      const part = fragments.pop(), start2 = state(), end2 = state();
      link(start2, part.start);
      link(part.end, part.start);
      link(part.end, end2);
      if (token === "*") link(start2, end2);
      fragments.push({ start: start2, end: end2 });
      quantified = true;
      continue;
    }
    quantified = false;
    if (token === "|" || token === "\xB7") {
      if (expectingAtom) throw new PatternError(`Unexpected token ${token}`);
      binary(token);
      expectingAtom = true;
    } else if (token === ")") {
      if (expectingAtom)
        throw new PatternError(
          "Expected a relation before closing parenthesis"
        );
      while (operators.length && operators.at(-1) !== "(") combine();
      if (operators.pop() !== "(")
        throw new PatternError("Unexpected closing parenthesis");
    } else {
      if (!expectingAtom) binary("\xB7");
      if (token === "(") {
        operators.push(token);
        expectingAtom = true;
      } else {
        if (!supported.has(token))
          throw new PatternError(
            `Unknown relation ${token}. Available: ${[...supported].join(", ")}`
          );
        const start2 = state(), end2 = state();
        states[start2].push({ kind: token, to: end2 });
        fragments.push({ start: start2, end: end2 });
        expectingAtom = false;
      }
    }
  }
  if (expectingAtom)
    throw new PatternError("Expected a relation at the end of the pattern");
  while (operators.length) {
    if (operators.at(-1) === "(")
      throw new PatternError("Expected a closing parenthesis");
    combine();
  }
  const { start, end } = fragments[0];
  return { start, end, states };
}
function distances(seeds, maxHops, expand) {
  let current = [...seeds];
  const best = new Map(current.map((seed) => [seed, 0]));
  for (let hops = 0; current.length; hops++) {
    const next = [];
    const offer = (to, cost) => {
      const distance = hops + cost;
      if (distance > maxHops || distance >= (best.get(to) ?? Infinity)) return;
      best.set(to, distance);
      (cost === 0 ? current : next).push(to);
    };
    for (const from of current)
      if (best.get(from) === hops) expand(from, hops, offer);
    current = next;
  }
  return best;
}
function traceRelations(index, seed, expression, maxHops = Infinity) {
  if (maxHops !== Infinity && (!Number.isSafeInteger(maxHops) || maxHops < 0))
    throw new PatternError("Maximum hops must be a nonnegative integer");
  const pattern = compile(expression, index.kinds);
  const visited = pattern.states.map(() => /* @__PURE__ */ new Map());
  function visit(node, state) {
    let found = visited[state].get(node);
    if (!found) {
      found = { node, state, incoming: [] };
      visited[state].set(node, found);
    }
    return found;
  }
  const first = visit(seed, pattern.start);
  const forward = distances([first], maxHops, (from, hops, offer) => {
    for (const move of pattern.states[from.state]) {
      if (!move.kind) {
        const to = visit(from.node, move.to);
        to.incoming.push({ from });
        offer(to, 0);
      } else if (hops < maxHops) {
        for (const edge of index.from(from.node, move.kind)) {
          const to = visit(edge.target, move.to);
          to.incoming.push({ from, edge });
          offer(to, 1);
        }
      }
    }
  });
  const accepted = visited[pattern.end], evidence = /* @__PURE__ */ new Set();
  distances(accepted.values(), maxHops, (to, suffix, offer) => {
    for (const { from, edge } of to.incoming) {
      const cost = edge ? 1 : 0;
      if (forward.get(from) + cost + suffix > maxHops) continue;
      if (edge) evidence.add(edge);
      offer(from, cost);
    }
  });
  return {
    endpoints: [...accepted.keys()],
    steps: [...evidence],
    visited: forward.size
  };
}
export {
  PatternError,
  RelationIndex,
  traceRelations
};
