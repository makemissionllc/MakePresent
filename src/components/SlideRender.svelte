<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import type { Look, Slide } from "../lib/types";
  import { fitText } from "../lib/fitText";
  import { hasChords, stripChords, parseChordLine } from "../lib/chords";

  interface Props {
    slide: Slide;
    look: Look;
    showText?: boolean;
    showBackground?: boolean;
    /** When true, Stage renders ChordPro bracketed chords as stacked band-view; Output always strips */
    isStage?: boolean;
  }

  let { slide, look, showText = true, showBackground = true, isStage = false }: Props = $props();

  const effectiveShowBackground = $derived(showBackground && look.showBackground);
  const effectiveShowText = $derived(showText);
  const shouldShowChords = $derived(isStage && hasChords(slide.body));

  function solidColor(s: Slide): string {
    return s.background.type === "solid" ? s.background.color : "#000000";
  }
</script>

<div
  class="slide-render"
  class:no-bg={!effectiveShowBackground}
  class:absolute={look.positioning === "absolute"}
  class:pos-top={look.textPosition === "top"}
  class:pos-center={look.textPosition === "center"}
  class:pos-bottom={look.textPosition === "bottom"}
  use:fitText={{
    mode: look.positioning === "absolute" ? "absolute" : "auto",
    deps: `${look.titleSize}:${look.bodySize}:${look.textPosition}:${look.positioning}:${look.titleFont}:${look.bodyFont}:${effectiveShowText}:${effectiveShowBackground}`,
  }}
  style:--look-title-size={`${look.titleSize}px`}
  style:--look-body-size={`${look.bodySize}px`}
  style:background-color={effectiveShowBackground ? solidColor(slide) : "transparent"}
  style:color={look.textColor}
>
  {#if effectiveShowBackground}
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
  {#if effectiveShowText && slide.title}
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
      {stripChords(slide.title)}
    </h1>
  {/if}
  {#if effectiveShowText && slide.body}
    {#if shouldShowChords}
      <div
        class="look-body chord-body"
        data-role="body"
        style:font-family={look.bodyFont}
        style:left={`${look.bodyBox.x}%`}
        style:top={`${look.bodyBox.y}%`}
        style:width={`${look.bodyBox.width}%`}
        style:height={`${look.bodyBox.height}%`}
        style:z-index={look.bodyBox.zIndex}
      >
        {#each slide.body.split("\n") as line}
          {#if line.trim() === ""}
            <div class="chord-line empty-line"><br /></div>
          {:else}
            {@const segments = parseChordLine(line)}
            <div class="chord-line">
              {#each segments as seg}
                <span class="chord-segment">
                  <span class="chord">{seg.chord ?? ""}</span>
                  <span class="lyric">{seg.lyric}</span>
                </span>
              {/each}
            </div>
          {/if}
        {/each}
      </div>
    {:else}
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
        {isStage ? slide.body : stripChords(slide.body)}
      </p>
    {/if}
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

  /* Stage — ChordPro stacked band view: chord above lyric, left-aligned to its word.
     Uses simple inline-flex per segment; no canvas measurement needed beyond fitText's
     existing scrollHeight check (reuses fitText patterns). For proportional fonts,
     left-edge alignment is visually correct; more sophisticated per-glyph measurement
     would only be needed for sub-pixel justification, flagged as future if needed. */
  .chord-body {
    display: flex;
    flex-direction: column;
    gap: 0.35em;
    align-items: center;
    white-space: pre-wrap;
  }
  .slide-render.pos-top .chord-body { align-items: flex-start; }
  .slide-render.pos-bottom .chord-body { align-items: flex-end; }
  .chord-line {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 0;
    line-height: 1.1;
    width: 100%;
  }
  .slide-render.pos-top .chord-line { justify-content: flex-start; }
  .slide-render.pos-bottom .chord-line { justify-content: flex-end; }
  .chord-segment {
    display: inline-flex;
    flex-direction: column;
    align-items: flex-start;
    vertical-align: bottom;
  }
  .chord {
    font-size: 0.52em;
    font-weight: 800;
    color: #fbbf24;
    line-height: 1;
    min-height: 0.9em;
    white-space: pre;
    text-shadow: 0 1px 10px rgba(0, 0, 0, 0.6);
    letter-spacing: 0.02em;
  }
  .lyric {
    white-space: pre;
    line-height: 1.15;
  }
  .chord:empty { min-height: 0; }
</style>
