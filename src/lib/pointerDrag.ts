export type PointerDragPoint = { x: number; y: number };

export type PointerDragCallbacks<T> = {
  onMove: (payload: T, point: PointerDragPoint, target: Element | null) => void;
  onDrop: (payload: T, point: PointerDragPoint, target: Element | null) => void;
  onCancel?: (payload: T) => void;
};

type ActiveDrag<T> = {
  pointerId: number;
  start: PointerDragPoint;
  point: PointerDragPoint;
  source: HTMLElement;
  payload: T;
  callbacks: PointerDragCallbacks<T>;
  ghost: HTMLElement;
  started: boolean;
};

let active: ActiveDrag<any> | null = null;
const THRESHOLD = 5;

function scrollNearEdge(target: Element | null, y: number): void {
  let node: Element | null = target;
  while (node && node !== document.body) {
    if (node instanceof HTMLElement) {
      const style = getComputedStyle(node);
      if (/(auto|scroll)/.test(`${style.overflowY} ${style.overflow}`) && node.scrollHeight > node.clientHeight) {
        const rect = node.getBoundingClientRect();
        if (y < rect.top + 38) node.scrollTop -= 14;
        else if (y > rect.bottom - 38) node.scrollTop += 14;
        return;
      }
    }
    node = node.parentElement;
  }
  if (y < 38) window.scrollBy(0, -14);
  else if (y > window.innerHeight - 38) window.scrollBy(0, 14);
}

function removeListeners<T>(drag: ActiveDrag<T>): void {
  window.removeEventListener("pointermove", move as EventListener, true);
  window.removeEventListener("pointerup", finish as EventListener, true);
  window.removeEventListener("pointercancel", cancel as EventListener, true);
  window.removeEventListener("keydown", keydown, true);
  drag.ghost.remove();
  drag.source.classList.remove("pointer-dragging");
  active = null;
}

function move(event: PointerEvent): void {
  const drag = active;
  if (!drag || event.pointerId !== drag.pointerId) return;
  drag.point = { x: event.clientX, y: event.clientY };
  if (!drag.started && Math.hypot(event.clientX - drag.start.x, event.clientY - drag.start.y) < THRESHOLD) return;
  drag.started = true;
  event.preventDefault();
  drag.ghost.style.transform = `translate(${event.clientX + 12}px, ${event.clientY + 12}px)`;
  drag.source.classList.add("pointer-dragging");
  const target = document.elementFromPoint(event.clientX, event.clientY);
  scrollNearEdge(target, event.clientY);
  drag.callbacks.onMove(drag.payload, drag.point, target);
}

function finish(event: PointerEvent): void {
  const drag = active;
  if (!drag || event.pointerId !== drag.pointerId) return;
  if (drag.started) {
    event.preventDefault();
    const suppressClick = (click: MouseEvent) => {
      click.preventDefault();
      click.stopImmediatePropagation();
      drag.source.removeEventListener("click", suppressClick, true);
    };
    drag.source.addEventListener("click", suppressClick, true);
    window.setTimeout(() => drag.source.removeEventListener("click", suppressClick, true), 0);
    const point = { x: event.clientX, y: event.clientY };
    drag.callbacks.onDrop(drag.payload, point, document.elementFromPoint(point.x, point.y));
  }
  removeListeners(drag);
}

function cancel(event: PointerEvent): void {
  const drag = active;
  if (!drag || event.pointerId !== drag.pointerId) return;
  drag.callbacks.onCancel?.(drag.payload);
  removeListeners(drag);
}

function keydown(event: KeyboardEvent): void {
  if (event.key !== "Escape" || !active) return;
  event.preventDefault();
  const drag = active;
  drag.callbacks.onCancel?.(drag.payload);
  removeListeners(drag);
}

export function beginPointerDrag<T>(event: PointerEvent, payload: T, callbacks: PointerDragCallbacks<T>): void {
  if (event.button !== 0 || active) return;
  const source = event.currentTarget;
  if (!(source instanceof HTMLElement)) return;
  const ghost = source.cloneNode(true) as HTMLElement;
  ghost.removeAttribute("id");
  ghost.setAttribute("aria-hidden", "true");
  Object.assign(ghost.style, {
    position: "fixed", left: "0", top: "0", margin: "0", pointerEvents: "none",
    zIndex: "99999", opacity: "0.82", width: `${source.getBoundingClientRect().width}px`,
    maxHeight: "70vh", overflow: "hidden", transform: `translate(${event.clientX + 12}px, ${event.clientY + 12}px)`,
  });
  document.body.append(ghost);
  active = {
    pointerId: event.pointerId,
    start: { x: event.clientX, y: event.clientY },
    point: { x: event.clientX, y: event.clientY },
    source, payload, callbacks, ghost, started: false,
  };
  window.addEventListener("pointermove", move, true);
  window.addEventListener("pointerup", finish, true);
  window.addEventListener("pointercancel", cancel, true);
  window.addEventListener("keydown", keydown, true);
}

export function cancelPointerDrag(): void {
  if (!active) return;
  active.callbacks.onCancel?.(active.payload);
  removeListeners(active);
}
