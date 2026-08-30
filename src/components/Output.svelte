<script lang="ts">
  import { onMount } from "svelte";
  import { api, subscribeState } from "../lib/sync";
  import type { ClientState } from "../lib/types";

  let appState = $state<ClientState | null>(null);

  const project = $derived(appState?.project ?? null);
  const live = $derived(
    project
      ? (project.slides.find((s) => s.id === project.live) ?? null)
      : null,
  );
  const color = $derived(
    live && live.background.type === "solid"
      ? live.background.color
      : "#000000",
  );

  onMount(() => {
    let un: () => void = () => {};
    void (async () => {
      un = await subscribeState((s) => {
        appState = s;
      });
      try {
        appState = await api.getState();
      } catch (e) {
        console.error("Failed to fetch initial appState", e);
      }
    })();
    return () => un();
  });
</script>

<main class="stage">
  {#if live}
    <div class="slide" style:background-color={color}>
      {#if live.title}
        <h1 class="title">{live.title}</h1>
      {/if}
      {#if live.body}
        <p class="body">{live.body}</p>
      {/if}
    </div>
  {:else}
    <div class="offline"></div>
  {/if}
</main>

<style>
  :global(html),
  :global(body),
  :global(#app) {
    height: 100%;
    margin: 0;
    overflow: hidden;
  }

  .stage {
    position: relative;
    width: 100vw;
    height: 100vh;
    background: #000;
  }

  .slide,
  .offline {
    position: absolute;
    inset: 0;
  }

  .slide {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2.5vh;
    padding: 8vh 10vw;
    text-align: center;
    color: #ffffff;
  }

  .title {
    font-family: system-ui, -apple-system, "Segoe UI", Ubuntu, Cantarell,
      sans-serif;
    font-size: clamp(2.5rem, 8vmin, 9rem);
    font-weight: 700;
    margin: 0;
    line-height: 1.1;
    text-shadow: 0 2px 24px rgba(0, 0, 0, 0.45);
  }

  .body {
    font-family: system-ui, -apple-system, "Segoe UI", Ubuntu, Cantarell,
      sans-serif;
    font-size: clamp(1.25rem, 4.5vmin, 5rem);
    font-weight: 400;
    margin: 0;
    max-width: 80%;
    line-height: 1.4;
    white-space: pre-wrap;
    text-shadow: 0 2px 20px rgba(0, 0, 0, 0.4);
  }
</style>