// Typed DOM lookups for the viewer: a page element the viewer needs is required, so a missing one throws here
// with its name instead of failing later on null.
type ElementType<T extends Element> = abstract new () => T;

/** The page element with this id, checked to be of `type` (HTMLElement by default). */
export function byId(id: string): HTMLElement;
export function byId<T extends HTMLElement>(
  id: string,
  type: ElementType<T>
): T;
export function byId(
  id: string,
  type: ElementType<HTMLElement> = HTMLElement
): HTMLElement {
  const el = document.getElementById(id);
  if (!(el instanceof type)) throw new Error(`The viewer page has no #${id}.`);
  return el;
}

/** The first element under `root` matching `selector`, checked to be of `type`. */
export function find<T extends Element>(
  root: ParentNode,
  selector: string,
  type: ElementType<T>
): T {
  const el = root.querySelector(selector);
  if (!(el instanceof type))
    throw new Error(`The viewer markup has no ${selector}.`);
  return el;
}

/** The closest element to an event's target matching `selector`, if the target is inside one. */
export function closest(
  target: EventTarget | null,
  selector: string
): HTMLElement | null {
  const el = target instanceof Element ? target.closest(selector) : null;
  return el instanceof HTMLElement ? el : null;
}
