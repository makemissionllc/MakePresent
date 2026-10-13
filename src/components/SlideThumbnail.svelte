<script lang="ts">
  import { onMount } from "svelte";
  import type { Background, Look, Overlay, Slide } from "../lib/types";
  import SlideRender from "./SlideRender.svelte";

  interface Props {
    slide: Slide;
    effectiveBackground?: Background;
    look: Look;
    showText?: boolean;
    showBackground?: boolean;
    overlay?: Overlay | null;
    overlays?: Overlay[];
    aspectRatio?: string;
    isStage?: boolean;
    enableCamera?: boolean;
    enableVideo?: boolean;
    fallbackColor?: string;
  }

  let {
    slide,
    effectiveBackground,
    look,
    showText = true,
    showBackground = true,
    overlay = null,
    overlays = [],
    aspectRatio,
    isStage = false,
    enableCamera = false,
    enableVideo = false,
    fallbackColor = "#000000",
  }: Props = $props();

  const CANVAS_WIDTH = 1280;
  const CANVAS_HEIGHT = 720;
  let viewport: HTMLDivElement;
  let scale = $state(1);
  let left = $state(0);
  let top = $state(0);
  let visible = $state(typeof IntersectionObserver === "undefined");
  let pageVisible = $state(typeof document === "undefined" || !document.hidden);

  onMount(() => {
    const update = () => {
      const width = viewport.clientWidth;
      const height = viewport.clientHeight;
      // A temporarily hidden thumbnail must not collapse its render scale.
      if (width <= 0 || height <= 0) return;
      const nextScale = Math.min(width / CANVAS_WIDTH, height / CANVAS_HEIGHT);
      if (!Number.isFinite(nextScale) || nextScale <= 0) return;
      scale = nextScale;
      left = (width - CANVAS_WIDTH * scale) / 2;
      top = (height - CANVAS_HEIGHT * scale) / 2;
    };
    const observer = typeof ResizeObserver !== "undefined" ? new ResizeObserver(update) : null;
    observer?.observe(viewport);
    // Only previews intersecting a scroll pane need text fitting or media.
    const intersection = typeof IntersectionObserver !== "undefined"
      ? new IntersectionObserver(([entry]) => { visible = entry.isIntersecting; }, { rootMargin: "120px" })
      : null;
    intersection?.observe(viewport);
    const onVisibility = () => { pageVisible = !document.hidden; };
    document.addEventListener("visibilitychange", onVisibility);
    if (!observer) window.addEventListener("resize", update);
    onVisibility();
    update();
    return () => {
      observer?.disconnect();
      intersection?.disconnect();
      document.removeEventListener("visibilitychange", onVisibility);
      if (!observer) window.removeEventListener("resize", update);
    };
  });
</script>

<div class="thumbnail-viewport" bind:this={viewport} style:background-color={fallbackColor}>
  <div
    class="thumbnail-canvas"
    style:left={`${left}px`}
    style:top={`${top}px`}
    style:transform={`scale(${scale})`}
  >
    {#if visible && pageVisible}
      <SlideRender
        {slide}
        {effectiveBackground}
        {look}
        {showText}
        {showBackground}
        {overlay}
        {overlays}
        {aspectRatio}
        {isStage}
        {enableCamera}
        {enableVideo}
        {fallbackColor}
      />
    {/if}
  </div>
</div>

<style>
  .thumbnail-viewport {
    position: absolute;
    inset: 0;
    overflow: hidden;
    background: #000;
  }
  .thumbnail-canvas {
    position: absolute;
    width: 1280px;
    height: 720px;
    transform-origin: top left;
    container-type: size;
  }
</style>
