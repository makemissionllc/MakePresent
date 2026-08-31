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
  class:pos-top={look.textPosition === "top"}
  class:pos-center={look.textPosition === "center"}
  class:pos-bottom={look.textPosition === "bottom"}
  use:fitText={{
    deps: `${look.titleSize}:${look.bodySize}:${look.textPosition}`,
  }}
  style:--look-title-size={`${look.titleSize}px`}
  style:--look-body-size={`${look.bodySize}px`}
  style:background-color={look.showBackground ? solidColor(slide) : "transparent"}
  style:color={look.textColor}
>
  {#if look.showBackground}
    {#if slide.background.type === "image"}
      <img
        class="media-layer"
        src={convertFileSrc(slide.background.path)}
        alt=""
        draggable="false"
        onerror={(e) => {
          (e.currentTarget as HTMLImageElement).style.display = "none";
        }}
      />
    {:else if slide.background.type === "video"}
      <video
        class="media-layer"
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
    <h1 class="look-title" data-role="title">{slide.title}</h1>
  {/if}
  {#if slide.body}
    <p class="look-body" data-role="body">{slide.body}</p>
  {/if}
</div>

<style>
  .slide-render {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2.5vh;
    padding: 8vh 10vw;
    text-align: center;
    overflow: hidden;
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
