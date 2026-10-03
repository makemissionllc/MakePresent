import { cubicOut } from "svelte/easing";
import type { TransitionConfig } from "svelte/transition";

/** Shared desktop motion. Keep interaction feedback brisk and let system
 * reduced-motion settings remove travel entirely. */
export function prefersReducedMotion(): boolean {
  return typeof window !== "undefined" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function veil(
  _node: Element,
  options: { duration?: number } = {},
): TransitionConfig {
  return {
    duration: prefersReducedMotion() ? 0 : options.duration ?? 180,
    easing: cubicOut,
    css: (t) => `opacity: ${t}`,
  };
}

export function surface(
  _node: Element,
  options: { duration?: number; distance?: number; scale?: number } = {},
): TransitionConfig {
  const distance = options.distance ?? 12;
  const scale = options.scale ?? 0.018;
  return {
    duration: prefersReducedMotion() ? 0 : options.duration ?? 260,
    easing: cubicOut,
    css: (t) => `opacity: ${t}; transform: translate3d(0, ${(1 - t) * distance}px, 0) scale(${1 - (1 - t) * scale})`,
  };
}
