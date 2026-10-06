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
    fallbackColor = "#000000",
  }: Props = $props();

  const CANVAS_WIDTH = 1280;
  const CANVAS_HEIGHT = 720;
  let viewport: HTMLDivElement;
  let scale = $state(1);
  let left = $state(0);
  let top = $state(0);

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
    window.addEventListener("resize", update);
    update();
    return () => { observer?.disconnect(); window.removeEventListener("resize", update); };
  });
</script>

<div class="thumbnail-viewport" bind:this={viewport} style:background-color={fallbackColor}>
  <div
    class="thumbnail-canvas"
    style:left={`${left}px`}
    style:top={`${top}px`}
    style:transform={`scale(${scale})`}
  >
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
      {fallbackColor}
    />
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
