<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import type { Look, Slide } from "../lib/types";
  import { fitText } from "../lib/fitText";

  interface Props {
    slide: Slide;
    look: Look;
  }

  let { slide, look }: Props = $props();

  function solidColor(s: Slide): string {
    return s.background.type === "solid" ? s.background.color : "#000000";
  }
</script>

<div
  class="slide-render"
  class:no-bg={!look.showBackground}
  class:absolute={look.positioning === "absolute"}
  class:pos-top={look.textPosition === "top"}
  class:pos-center={look.textPosition === "center"}
  class:pos-bottom={look.textPosition === "bottom"}
  use:fitText={{
    mode: look.positioning === "absolute" ? "absolute" : "auto",
    deps: `${look.titleSize}:${look.bodySize}:${look.textPosition}:${look.positioning}:${look.titleFont}:${look.bodyFont}`,
  }}
  style:--look-title-size={`${look.titleSize}px`}
  style:--look-body-size={`${look.bodySize}px`}
  style:background-color={look.showBackground ? solidColor(slide) : "transparent"}
  style:color={look.textColor}
>
  {#if look.showBackground}
    {#if slide.background.type === "image"}
      <img
        class="slide-background media-layer"
        src={convertFileSrc(slide.background.path)}
        alt=""
        draggable="false"
        onerror={(e) => {
          (e.currentTarget as HTMLImageElement).style.display = "none";
        }}
      />
    {:else if slide.background.type === "video"}
      <video
        class="slide-background media-layer"
        src={convertFileSrc(slide.background.path)}
        autoplay
        loop
        muted
        playsinline
        preload="auto"
      ></video>
    {/if}
  {/if}
  {#if slide.title}
    <h1
      class="look-title"
      data-role="title"
      style:font-family={look.titleFont}
      style:left={`${look.titleBox.x}%`}
      style:top={`${look.titleBox.y}%`}
      style:width={`${look.titleBox.width}%`}
      style:height={`${look.titleBox.height}%`}
      style:z-index={look.titleBox.zIndex}
    >
      {slide.title}
    </h1>
  {/if}
  {#if slide.body}
    <p
      class="look-body"
      data-role="body"
      style:font-family={look.bodyFont}
      style:left={`${look.bodyBox.x}%`}
      style:top={`${look.bodyBox.y}%`}
      style:width={`${look.bodyBox.width}%`}
      style:height={`${look.bodyBox.height}%`}
      style:z-index={look.bodyBox.zIndex}
    >
      {slide.body}
    </p>
  {/if}
</div>

<style>
  .slide-render {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    box-sizing: border-box;
    gap: 2.5vh;
    padding: 8vh 10vw;
    text-align: center;
    overflow: hidden;
  }

  .slide-background {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    z-index: 0;
    object-fit: cover;
  }

  .slide-render.pos-top {
    justify-content: flex-start;
  }

  .slide-render.pos-center {
    justify-content: center;
  }

  .slide-render.pos-bottom {
    justify-content: flex-end;
  }

  .media-layer {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .look-title,
  .look-body {
    position: relative;
    z-index: 1;
  }

  /* FreeShow-style absolute layout: each text role becomes an explicit,
     draggable bounding box positioned by the geometry injected as inline
     styles (left/top/width/height/z-index) from the Look. */
  .slide-render.absolute .look-title,
  .slide-render.absolute .look-body {
    position: absolute;
    display: flex;
    flex-direction: column;
    justify-content: center;
    box-sizing: border-box;
    margin: 0;
    overflow: hidden;
    text-align: center;
    white-space: pre-wrap;
  }
  .slide-render.absolute .look-body {
    max-width: none;
  }

  .look-title {
    font-family: var(--font-display);
    font-size: var(--look-title-size, clamp(2.5rem, 8vmin, 9rem));
    font-weight: 400;
    margin: 0;
    line-height: 1.1;
    text-shadow: 0 2px 24px rgba(0, 0, 0, 0.45);
  }

  .look-body {
    font-family: var(--font-body);
    font-size: var(--look-body-size, clamp(1.25rem, 4.5vmin, 5rem));
    font-weight: 400;
    margin: 0;
    max-width: 80%;
    line-height: 1.4;
    white-space: pre-wrap;
    text-shadow: 0 2px 20px rgba(0, 0, 0, 0.4);
  }
</style>
