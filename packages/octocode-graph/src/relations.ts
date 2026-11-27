/** A single attributed relation. Separate occurrences, including parallel edges, survive traversal. */
export interface Relation {
  relation: string;
  source: string;
  target: string;
  path: string;
  lines: readonly number[];
  via?: string;
  confidence?: string;
  count?: number;
  test?: boolean;
}

export class RelationIndex {
  private readonly outgoing = new Map<string, Map<string, Relation[]>>();
  private readonly endpoints = new Set<string>();
  readonly kinds: ReadonlySet<string>;

  constructor(kinds: Iterable<string>, relations: Iterable<Relation>) {
    this.kinds = new Set(kinds);
    for (const edge of relations) {
      if (!this.kinds.has(edge.relation))
        throw new Error(`Undeclared relation ${edge.relation}`);
      this.endpoints.add(edge.source);
      this.endpoints.add(edge.target);
      const neighbors =
        this.outgoing.get(edge.source) ?? new Map<string, Relation[]>();
      const edges = neighbors.get(edge.relation) ?? [];
      edges.push(edge);
      neighbors.set(edge.relation, edges);
      this.outgoing.set(edge.source, neighbors);
    }
  }

  has(node: string) {
    return this.endpoints.has(node);
  }

  from(node: string, kind: string): readonly Relation[] {
    return this.outgoing.get(node)?.get(kind) ?? [];
  }
}

type Transition = { to: number; kind?: string };
type Fragment = { start: number; end: number };

export class PatternError extends Error {}

// Thompson construction. Each operator adds a constant number of epsilon transitions.
function compile(expression: string, supported: ReadonlySet<string>) {
  const tokens = expression.match(/[A-Za-z_][A-Za-z_0-9]*|[^\s]/gu) ?? [];
  const states: Transition[][] = [];
  const state = () => states.push([]) - 1;
  const link = (a: number, b: number) => states[a].push({ to: b });
  const fragments: Fragment[] = [];
  const operators: string[] = [];
  const precedence = (op: string) => (op === '·' ? 2 : op === '|' ? 1 : 0);

  function combine() {
    const op = operators.pop();
    const right = fragments.pop()!;
    const left = fragments.pop()!;
    if (op === '·') {
      link(left.end, right.start);
      fragments.push({ start: left.start, end: right.end });
    } else {
      const start = state(),
        end = state();
      link(start, left.start);
      link(start, right.start);
      link(left.end, end);
      link(right.end, end);
      fragments.push({ start, end });
    }
  }

  function binary(op: string) {
    while (operators.length && precedence(operators.at(-1)!) >= precedence(op))
      combine();
    operators.push(op);
  }

  let expectingAtom = true;
  let quantified = false;
  for (const token of tokens) {
    if (token === '*' || token === '+') {
      if (expectingAtom || quantified)
        throw new PatternError(`Unexpected token ${token}`);
      const part = fragments.pop()!,
        start = state(),
        end = state();
      link(start, part.start);
      link(part.end, part.start);
      link(part.end, end);
      if (token === '*') link(start, end);
      fragments.push({ start, end });
      quantified = true;
      continue;
    }
    quantified = false;
    if (token === '|' || token === '·') {
      if (expectingAtom) throw new PatternError(`Unexpected token ${token}`);
      binary(token);
      expectingAtom = true;
    } else if (token === ')') {
      if (expectingAtom)
        throw new PatternError(
          'Expected a relation before closing parenthesis'
        );
      while (operators.length && operators.at(-1) !== '(') combine();
      if (operators.pop() !== '(')
        throw new PatternError('Unexpected closing parenthesis');
    } else {
      if (!expectingAtom) binary('·');
      if (token === '(') {
        operators.push(token);
        expectingAtom = true;
      } else {
        if (!supported.has(token))
          throw new PatternError(
            `Unknown relation ${token}. Available: ${[...supported].join(', ')}`
          );
        const start = state(),
          end = state();
        states[start].push({ kind: token, to: end });
        fragments.push({ start, end });
        expectingAtom = false;
      }
    }
  }
  if (expectingAtom)
    throw new PatternError('Expected a relation at the end of the pattern');
  while (operators.length) {
    if (operators.at(-1) === '(')
      throw new PatternError('Expected a closing parenthesis');
    combine();
  }
  const { start, end } = fragments[0];
  return { start, end, states };
}

interface Visit {
  node: string;
  state: number;
  incoming: { from: Visit; edge?: Relation }[];
}

// Epsilon transitions cost zero and traversing one relation costs one. Finish the
// epsilon closure of each distance before advancing to the following frontier.
function distances(
  seeds: Iterable<Visit>,
  maxHops: number,
  expand: (
    from: Visit,
    hops: number,
    offer: (to: Visit, cost: 0 | 1) => void
  ) => void
) {
  let current = [...seeds];
  const best = new Map(current.map(seed => [seed, 0]));
  for (let hops = 0; current.length; hops++) {
    const next: Visit[] = [];
    const offer = (to: Visit, cost: 0 | 1) => {
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

/** Endpoints and all distinct witnesses on accepting walks within an optional hop budget. */
export function traceRelations(
  index: RelationIndex,
  seed: string,
  expression: string,
  maxHops = Infinity
) {
  if (maxHops !== Infinity && (!Number.isSafeInteger(maxHops) || maxHops < 0))
    throw new PatternError('Maximum hops must be a nonnegative integer');
  const pattern = compile(expression, index.kinds);
  const visited = pattern.states.map(() => new Map<string, Visit>());
  function visit(node: string, state: number): Visit {
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
  const accepted = visited[pattern.end],
    evidence = new Set<Relation>();
  // A prefix and suffix may each fit the budget while their concatenation does
  // not. Charge both shortest distances before accepting the witness.
  distances(accepted.values(), maxHops, (to, suffix, offer) => {
    for (const { from, edge } of to.incoming) {
      const cost = edge ? 1 : 0;
      if (forward.get(from)! + cost + suffix > maxHops) continue;
      if (edge) evidence.add(edge);
      offer(from, cost);
    }
  });
  return {
    endpoints: [...accepted.keys()],
    steps: [...evidence],
    visited: forward.size,
  };
}
