<script lang="ts">
  import { onMount } from "svelte";
  import type { Look, Overlay, Slide } from "../lib/types";
  import SlideRender from "./SlideRender.svelte";

  interface Props {
    slide: Slide;
    look: Look;
    showText?: boolean;
    showBackground?: boolean;
    overlay?: Overlay | null;
    aspectRatio?: string;
    isStage?: boolean;
    enableCamera?: boolean;
  }

  let {
    slide,
    look,
    showText = true,
    showBackground = true,
    overlay = null,
    aspectRatio,
    isStage = false,
    enableCamera = false,
  }: Props = $props();

  const CANVAS_WIDTH = 1280;
  const CANVAS_HEIGHT = 720;
  let viewport: HTMLDivElement;
  let scale = $state(0);
  let left = $state(0);
  let top = $state(0);

  onMount(() => {
    const update = () => {
      const width = viewport.clientWidth;
      const height = viewport.clientHeight;
      const nextScale = Math.min(width / CANVAS_WIDTH, height / CANVAS_HEIGHT);
      scale = Number.isFinite(nextScale) ? nextScale : 0;
      left = (width - CANVAS_WIDTH * scale) / 2;
      top = (height - CANVAS_HEIGHT * scale) / 2;
    };
    const observer = new ResizeObserver(update);
    observer.observe(viewport);
    update();
    return () => observer.disconnect();
  });
</script>

<div class="thumbnail-viewport" bind:this={viewport}>
  <div
    class="thumbnail-canvas"
    style:left={`${left}px`}
    style:top={`${top}px`}
    style:transform={`scale(${scale})`}
  >
    <SlideRender
      {slide}
      {look}
      {showText}
      {showBackground}
      {overlay}
      {aspectRatio}
      {isStage}
      {enableCamera}
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
