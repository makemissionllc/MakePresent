<script lang="ts">
  import { onMount } from "svelte";
  import BrandLockup from "./BrandLockup.svelte";

  interface Props {
    displayCount: number | null;
    onFinish: (action: "view" | "tour" | "settings" | "skip") => void;
  }
  let { displayCount, onFinish }: Props = $props();
  let dialog: HTMLDialogElement;
  let heading: HTMLHeadingElement;
  let step = $state(0);
  const labels = ["Welcome", "Your workspace", "Ready to present"];

  onMount(() => {
    const previous = document.activeElement;
    dialog.showModal();
    heading.focus();
    return () => {
      dialog.close();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  });

  function goTo(next: number): void {
    step = next;
    heading.focus();
  }
</script>

<dialog bind:this={dialog} aria-labelledby="welcome-title" oncancel={(e) => { e.preventDefault(); onFinish("skip"); }} onkeydown={(e) => e.stopPropagation()}>
  <header>
    <BrandLockup />
    <button class="quiet" aria-label="Close getting started" onclick={() => onFinish("skip")}>×</button>
  </header>
  <div class="content">
    <nav aria-label="Getting started steps">
      {#each labels as label, i}
        <button class:current={step === i} aria-current={step === i ? "step" : undefined} onclick={() => goTo(i)}>
          <span class="step-number">{i + 1}</span><span>{label}</span>
        </button>
      {/each}
    </nav>
    <section>
      <p class="eyebrow">GETTING STARTED · {step + 1} OF 3</p>
      <h1 id="welcome-title" bind:this={heading} tabindex="-1">{step === 0 ? "Make room for the message." : step === 1 ? "A place for every part of your service." : "Your message. On the right screen."}</h1>
      {#if step === 0}
        <p class="intro">Lyrics, Scripture, and visuals. Together in one calm workspace, ready for the people you serve.</p>
        <div class="preview" aria-label="Illustration of a presentation playlist and Output">
          <div class="sample-playlist"><span>YOUR SERVICE</span><span class="sample-row">01 <b>Welcome</b></span><span class="sample-row selected">02 <b>Worship</b></span><span class="sample-row">03 <b>Message</b></span></div>
          <div class="sample-output"><span>Gather with purpose.</span><small>Make every moment meaningful.</small></div>
        </div>
        <p class="note">Start with a playlist, make it your own, and present when you’re ready. No account needed.</p>
      {:else if step === 1}
        <p class="intro">A View is your service workspace. Its playlist holds everything in the order you’ll present it.</p>
        <ol class="guide">
          <li><span class="guide-icon">1</span><div><strong>Build your playlist</strong><p>Choose a starting playlist or begin blank. Add songs, verses, and media, then drag to reorder.</p></div></li>
          <li><span class="guide-icon">2</span><div><strong>Give it a Look</strong><p>Use Looks to set fonts, backgrounds, and layouts for your Output and Stage Display.</p></div></li>
          <li><span class="guide-icon">3</span><div><strong>Keep your place</strong><p>Your current View saves automatically. Save a reusable playlist before creating your next View.</p></div></li>
        </ol>
        <p class="note">Once Output is visible, clicking a playlist slide puts it live. Use ← and → to move through the service when you’re not typing.</p>
      {:else}
        <p class="intro">Keep controls on your screen and the presentation on your projector or audience display.</p>
        <div class="display-status"><span class="status-dot"></span><div><strong>{displayCount === null ? "Check your displays in the editor" : displayCount > 1 ? `${displayCount} displays detected` : "Working with one display"}</strong><p>{displayCount !== null && displayCount > 1 ? "Choose the audience display in the Output panel, then select Show Output." : "You can prepare your service now. Connect a second display when you’re ready to present."}</p></div></div>
        <div class="connection-notes"><strong>Streaming with OBS?</strong><p>Enable NDI in Settings, show Output, then select MakrStudio in an OBS NDI Source. OBS needs the DistroAV plugin and a compatible NDI Runtime. Check for Live status in MakrStudio.</p></div>
        <p class="note">Showing Output can reveal the current live slide. Check the preview before putting it on screen.</p>
        <div class="next-actions"><button onclick={() => onFinish("tour")}>Take the workspace tour</button><button onclick={() => onFinish("settings")}>Open settings</button></div>
      {/if}
    </section>
  </div>
  <footer>
    <button class="quiet skip" onclick={() => onFinish("skip")}>Skip for now</button>
    <span class="footer-note">Revisit anytime in Help</span>
    <div class="navigation">
      {#if step > 0}<button onclick={() => goTo(step - 1)}>Back</button>{/if}
      <button class="primary" onclick={() => step < 2 ? goTo(step + 1) : onFinish("view")}>{step < 2 ? "Continue" : "Create a View"}<span aria-hidden="true"> →</span></button>
    </div>
  </footer>
</dialog>

<style>
  dialog { width: min(820px, calc(100vw - 32px)); max-height: calc(100dvh - 32px); padding: 0; color: var(--text); background: var(--panel); border: 1px solid var(--border); border-radius: 16px; box-shadow: 0 24px 80px #0008; overflow: auto; }
  dialog::backdrop { background: #080d10bf; }
  header, footer { display: flex; align-items: center; gap: 12px; padding: 20px 24px; }
  header { justify-content: space-between; border-bottom: 1px solid var(--border); }
  button { font: inherit; font-size: 13px; color: var(--text); background: var(--panel-2); border: 1px solid var(--border); border-radius: 7px; padding: 9px 13px; cursor: pointer; }
  button:hover { border-color: var(--accent); }
  .quiet { background: transparent; border-color: transparent; color: var(--text-dim); }
  header .quiet { font-size: 22px; padding: 0; width: 32px; height: 32px; }
  .content { display: grid; grid-template-columns: 176px minmax(0, 1fr); }
  nav { padding: 24px 12px; border-right: 1px solid var(--border); background: var(--bg); }
  nav button { display: flex; align-items: center; gap: 8px; width: 100%; text-align: left; background: transparent; border-color: transparent; color: var(--text-dim); margin-bottom: 8px; padding: 10px 8px; font-size: 12px; }
  nav button.current { color: var(--text); background: var(--panel-2); }
  .step-number { display: grid; place-items: center; width: 21px; height: 21px; flex: none; border: 1px solid var(--border); border-radius: 50%; font-size: 10px; }
  .current .step-number { color: var(--bg); background: var(--accent); border-color: var(--accent); }
  section { padding: 30px; min-height: 380px; }
  .eyebrow { margin: 0 0 12px; font-size: 10px; font-weight: 600; letter-spacing: .14em; color: var(--accent); }
  h1 { margin: 0; font-family: var(--font-body); font-size: clamp(22px, 3vw, 30px); line-height: 1.2; font-weight: 600; letter-spacing: -.04em; outline: none; }
  p { font-size: 13px; line-height: 1.6; color: var(--text-dim); }
  .intro { margin: 14px 0 22px; }
  .preview { display: grid; grid-template-columns: 118px minmax(0, 1fr); padding: 12px; gap: 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--bg); }
  .sample-playlist { display: flex; flex-direction: column; gap: 8px; color: var(--text-dim); font-size: 9px; }
  .sample-playlist > span:first-child { padding: 5px; font-size: 8px; letter-spacing: .08em; }
  .sample-row { padding: 8px; border-radius: 5px; }
  .sample-row b { margin-left: 8px; color: var(--text); font-weight: 500; }
  .selected { background: #81aa951a; outline: 1px solid #81aa9550; }
  .sample-output { min-height: 148px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; padding: 20px; background: #263c32; border-radius: 5px; text-align: center; }
  .sample-output span { color: #edf4ed; font-size: 22px; font-weight: 600; letter-spacing: -.04em; }
  .sample-output small { font-size: 10px; color: #becfc4; }
  .note { font-size: 12px; margin: 18px 0 0; }
  .guide { display: flex; flex-direction: column; gap: 20px; list-style: none; margin: 0; padding: 0; }
  .guide li, .display-status { display: flex; gap: 12px; }
  .guide-icon { width: 28px; height: 28px; display: grid; place-items: center; flex: none; background: var(--panel-2); border-radius: 7px; color: var(--accent); font-size: 12px; }
  strong { font-size: 13px; font-weight: 600; }
  .guide p, .display-status p, .connection-notes p { margin: 4px 0 0; font-size: 12px; }
  .display-status { padding: 16px; border: 1px solid var(--border); border-radius: 9px; background: var(--panel-2); }
  .status-dot { width: 7px; height: 7px; flex: none; margin-top: 6px; border-radius: 50%; background: var(--accent); }
  .connection-notes { margin-top: 20px; }
  .next-actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 16px; }
  .next-actions button { padding: 7px 10px; font-size: 12px; background: transparent; }
  footer { border-top: 1px solid var(--border); flex-wrap: wrap; }
  .footer-note { font-size: 11px; color: var(--text-dim); }
  .navigation { display: flex; gap: 8px; margin-left: auto; }
  .primary { background: var(--accent); border-color: var(--accent); color: #142219; font-weight: 600; }
  .primary:hover { filter: brightness(1.08); }
  @media (max-width: 620px) {
    .content { grid-template-columns: minmax(0, 1fr); }
    nav { display: flex; gap: 4px; padding: 10px; border-right: 0; border-bottom: 1px solid var(--border); }
    nav button { margin: 0; padding: 6px; justify-content: center; }
    nav button span:last-child { display: none; }
    section { padding: 24px; min-height: 0; }
    header, footer { padding: 16px; }
    .footer-note { display: none; }
    .preview { grid-template-columns: minmax(0, 1fr); }
    .sample-playlist { display: none; }
  }
</style>
