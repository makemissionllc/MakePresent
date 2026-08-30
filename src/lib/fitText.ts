import type { Action } from "svelte/action";

/**
 * Auto-fit slide text so long content shrinks to stay fully visible instead of
 * overflowing or clipping. Pure rendering concern, local to each window — no
 * backend/state involvement.
 *
 * Attach to the element that CONTAINS the text blocks:
 *
 *   <div class="slide" use:fitText>          <section class="current" use:fitText>
 *     <h1 data-role="title">…</h1>            <p  data-role="body">…</p>
 *     <p  data-role="body">…</p>
 *   </div>
 *
 * Behaviour
 * - Title and body are measured independently (each gets its own binary search
 *   on font-size), sharing only the vertical budget of the container.
 * - Measurement runs with the block width pinned to its allowed width, so a
 *   single unbreakably long "word" (URL, chemical formula, …) is caught by the
 *   scrollWidth check and shrinks rather than spilling out.
 * - Body text is measured as a whole multi-line block, not line by line.
 * - If the text already fits at its natural CSS size (a clamp();vmin default),
 *   nothing is overridden — short slides keep their default size, never a
 *   needless shrink.
 * - Below the per-kind minimum size text truncates with an ellipsis
 *   (-webkit-line-clamp) rather than becoming illegibly tiny.
 * - Recomputes on initial mount, on container resize (ResizeObserver — covers
 *   window/display resizes and vmin base changes), and on content changes
 *   (MutationObserver — new slide live, edits). It does NOT run per animation
 *   frame, so the 400 ms crossfade and any other animation never trigger a
 *   re-measure.
 */

export interface FitTextOptions {
  /** Floor font size in px for [data-role="title"] before ellipsis truncation. */
  minTitlePx?: number;
  /** Floor font size in px for [data-role="body"] before ellipsis truncation. */
  minBodyPx?: number;
}

const TITLE_SEL = '[data-role="title"]';
const BODY_SEL = '[data-role="body"]';
const EPS = 1;

interface Metrics {
  basePx: number;
  lineHeightPx: number;
  maxWidthPct: number | null;
  naturalClamp: number;
}

function px(value: string, fallback = 0): number {
  const n = parseFloat(value);
  return Number.isFinite(n) && n >= 0 ? n : fallback;
}

// Measurements must happen in plain block flow (un-clamped, un-hidden), else a
// pre-existing -webkit-line-clamp would hide overflowing lines and the element
// would report "fits" by definition. Everything below is synchronous, so the
// browser cannot paint the transient state.
function neutralise(el: HTMLElement): void {
  el.style.display = "block";
  el.style.overflow = "visible";
  el.style.whiteSpace = "";
  el.style.webkitBoxOrient = "";
  el.style.webkitLineClamp = "none";
  el.style.width = "";
  el.style.fontSize = "";
  el.style.lineHeight = "";
  el.style.textOverflow = "";
}

function restoreNatural(el: HTMLElement): void {
  el.style.display = "";
  el.style.overflow = "";
  el.style.whiteSpace = "";
  el.style.webkitBoxOrient = "";
  el.style.webkitLineClamp = "";
  el.style.width = "";
  el.style.fontSize = "";
  el.style.lineHeight = "";
  el.style.textOverflow = "";
}

function readMetrics(el: HTMLElement): Metrics {
  const cs = getComputedStyle(el);
  const basePx = px(cs.fontSize) || 16;
  const lineHeightPx = px(cs.lineHeight, basePx * 1.25);
  const maxWidthPct = cs.maxWidth.endsWith("%") ? px(cs.maxWidth, 100) : null;
  const naturalClamp = Math.max(0, parseInt(cs.webkitLineClamp, 10) || 0);
  return { basePx, lineHeightPx, maxWidthPct, naturalClamp };
}

function fits(el: HTMLElement, allowedW: number, allowedH: number): boolean {
  return (
    el.scrollWidth <= allowedW + EPS && el.scrollHeight <= allowedH + EPS
  );
}

/**
 * Size ONE element to its largest fitting font size. Returns its resulting
 * rendered height so siblings can budget against it.
 */
function fitElement(
  el: HTMLElement,
  minPx: number,
  allowedW: number,
  allowedH: number,
): number {
  neutralise(el);
  const m = readMetrics(el);
  el.style.width = `${allowedW}px`;

  let sizePx: number;
  let trunc = false;

  const fitsAt = (s: number): boolean => {
    el.style.fontSize = `${s}px`;
    return fits(el, allowedW, allowedH);
  };

  if (fitsAt(m.basePx)) {
    sizePx = m.basePx;
  } else {
    // Largest size in [minPx, base) that fits. Font-size monotonicity makes
    // this a clean binary search (~6-8 reflows per element per pass).
    let lo = Math.ceil(minPx);
    let hi = Math.floor(m.basePx);
    let best = -1;
    while (lo <= hi) {
      const mid = Math.round((lo + hi) / 2);
      if (fitsAt(mid)) {
        best = mid;
        lo = mid + 1;
      } else {
        hi = mid - 1;
      }
    }
    if (best >= Math.ceil(minPx)) {
      sizePx = best;
    } else {
      sizePx = Math.min(minPx, m.basePx);
      trunc = true;
    }
  }

  // Pin the measured geometry so subsequent layout matches the fit exactly.
  el.style.width = `${allowedW}px`;
  el.style.fontSize = `${sizePx}px`;

  const lineBox = (m.lineHeightPx * sizePx) / m.basePx;
  const measuredLines = Math.max(1, Math.ceil(el.scrollHeight / lineBox));

  if (trunc) {
    // Even the floor size overflows: clamp to what the slot fits, ellipsis on.
    const lines = Math.max(1, Math.floor(allowedH / lineBox));
    el.style.display = "-webkit-box";
    el.style.webkitBoxOrient = "vertical";
    el.style.overflow = "hidden";
    el.style.webkitLineClamp = String(lines);
  } else if (m.naturalClamp > 0 && measuredLines > m.naturalClamp) {
    // Natural CSS already peeks via a line-clamp that would re-cut the now
    // fitted content: raise the clamp so everything stays visible.
    el.style.display = "-webkit-box";
    el.style.webkitBoxOrient = "vertical";
    el.style.overflow = "hidden";
    el.style.webkitLineClamp = String(measuredLines);
  } else if (sizePx === m.basePx) {
    // Fits at the natural default size and nothing needed clamping: leave the
    // stylesheet completely alone so default sizing is never disturbed.
    restoreNatural(el);
    return el.offsetHeight;
  }

  return el.offsetHeight;
}

export const fitText: Action<HTMLElement, FitTextOptions | undefined> = (
  node,
  opts,
) => {
  const minTitlePx = opts?.minTitlePx ?? 24;
  const minBodyPx = opts?.minBodyPx ?? 16;
  let raf = 0;

  const run = (): void => {
    const title = node.querySelector<HTMLElement>(TITLE_SEL);
    const body = node.querySelector<HTMLElement>(BODY_SEL);
    if (!title && !body) return;

    const cs = getComputedStyle(node);
    const padX = px(cs.paddingLeft) + px(cs.paddingRight);
    const padY = px(cs.paddingTop) + px(cs.paddingBottom);
    const gapRaw = cs.rowGap || cs.gap || "";
    const gap = gapRaw !== "" && gapRaw !== "normal" ? px(gapRaw) : 0;
    const contentW = Math.max(0, node.clientWidth - padX);
    const contentH = Math.max(0, node.clientHeight - padY);

    const wFor = (el: HTMLElement): number => {
      const pct = readMetrics(el).maxWidthPct ?? 100;
      return Math.max(0, ((contentW * pct) / 100));
    };

    let titleH = title ? title.offsetHeight : 0;
    let bodyH = body ? body.offsetHeight : 0;
    for (let pass = 0; pass < 3; pass++) {
      let tH = 0;
      let bH = 0;
      if (title) {
        tH = fitElement(
          title,
          minTitlePx,
          wFor(title),
          Math.max(0, contentH - gap - bodyH),
        );
      }
      if (body) {
        bH = fitElement(
          body,
          minBodyPx,
          wFor(body),
          Math.max(0, contentH - gap - titleH),
        );
      }
      if (pass > 0 && tH === titleH && bH === bodyH) break;
      titleH = tH;
      bodyH = bH;
    }
  };

  // Coalesce all triggers (mount, resize, content change) to at most one
  // measure per frame; never a per-frame loop during transitions.
  const schedule = (): void => {
    if (!raf) {
      raf = requestAnimationFrame(() => {
        raf = 0;
        run();
      });
    }
  };

  const ro = new ResizeObserver(schedule);
  ro.observe(node);
  const mo = new MutationObserver(schedule);
  mo.observe(node, { childList: true, characterData: true, subtree: true });

  schedule();
  if (document.fonts?.ready) {
    void document.fonts.ready.then(schedule).catch(() => {});
  }

  return {
    destroy(): void {
      ro.disconnect();
      mo.disconnect();
      if (raf) cancelAnimationFrame(raf);
    },
  };
};