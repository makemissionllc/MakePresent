<script lang="ts">
  // Live camera / capture-card feed via the webview's native getUserMedia.
  // No Rust involvement: capture cards presenting as UVC webcams are visible
  // to standard browser APIs. The stream starts on mount and ALL tracks stop
  // on unmount, so a device is only ever open while its slide is rendered
  // with the feed enabled (Output live/leaving frames, Editor live preview).
  // Always muted: camera audio is a PA/mixer concern, consistent with the
  // muted recorded-video backgrounds.

  interface Props {
    /** Exact browser media-device id; null = match by label, then default camera. */
    deviceId?: string | null;
    /** Human device name for status text + label fallback matching. */
    label: string;
  }

  let { deviceId = null, label }: Props = $props();

  let videoEl = $state<HTMLVideoElement | null>(null);
  let live = $state(false);
  let error = $state<string | null>(null);

  function describe(e: unknown): string {
    const err = e as { name?: string; message?: string } | null;
    const name = err?.name ?? "error";
    if (name === "NotAllowedError" || name === "SecurityError")
      return "Camera access denied — allow camera access for MakrStudio, then re-select the camera.";
    if (name === "NotFoundError" || name === "OverconstrainedError")
      return `Camera "${label || "selected"}" not found — still plugged in? Re-select it as the background.`;
    if (name === "NotReadableError")
      return `Camera "${label || "selected"}" is busy in another app — close it there first.`;
    return `Camera unavailable (${name}): ${err?.message ?? String(e)}`;
  }

  async function openStream(signal: { cancelled: boolean }): Promise<MediaStream> {
    if (!navigator.mediaDevices?.getUserMedia || !navigator.mediaDevices?.enumerateDevices)
      throw { name: "NotSupportedError", message: "this window cannot capture cameras" };
    // Prefer the exact device id; fall back to label (ids can rotate across
    // restarts), then to the default camera.
    let exact: string | undefined;
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      const cams = devices.filter((d) => d.kind === "videoinput");
      if (deviceId && cams.some((c) => c.deviceId === deviceId)) exact = deviceId;
      else if (label) exact = cams.find((c) => c.label === label)?.deviceId;
    } catch {
      // Enumeration failed — still try the default camera below.
    }
    const constraints = (id: string | undefined): MediaStreamConstraints =>
      id
        ? { video: { deviceId: { exact: id } }, audio: false }
        : { video: true, audio: false };
    try {
      return await navigator.mediaDevices.getUserMedia(constraints(exact));
    } catch (e) {
      // Exact match failed (device unplugged since selection) — one retry on
      // the default camera before surfacing an error.
      if (exact) return await navigator.mediaDevices.getUserMedia(constraints(undefined));
      throw e;
    }
  }

  $effect(() => {
    // Track device identity so re-selecting a camera restarts the stream.
    const id = deviceId;
    const name = label;
    void id;
    void name;
    let cancelled = false;
    let stream: MediaStream | null = null;
    live = false;
    error = null;
    void (async () => {
      try {
        stream = await openStream({ cancelled });
        if (cancelled) {
          stream.getTracks().forEach((t) => t.stop());
          return;
        }
        if (videoEl) {
          videoEl.srcObject = stream;
          await videoEl.play().catch(() => {});
        }
        if (!cancelled) live = true;
      } catch (e) {
        if (!cancelled) error = describe(e);
      }
    })();
    return () => {
      cancelled = true;
      stream?.getTracks().forEach((t) => t.stop());
      if (videoEl) videoEl.srcObject = null;
    };
  });
</script>

<video
  bind:this={videoEl}
  class="camera-video"
  autoplay
  muted
  playsinline
  aria-label={label ? `Live camera: ${label}` : "Live camera"}
></video>
{#if !live}
  <div class="camera-status" role="status">
    {#if error}
      <span class="camera-error">{error}</span>
    {:else}
      <span>Connecting to {label || "camera"}…</span>
    {/if}
  </div>
{/if}

<style>
  /* Same full-bleed object-fit:cover treatment as recorded video backgrounds
     (SlideRender duplicates these rules because Svelte scopes styles per
     component). */
  .camera-video {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .camera-status {
    position: absolute;
    inset: auto 0 0 0;
    display: flex;
    justify-content: center;
    padding: 6px 10px;
    font-size: 11px;
    line-height: 1.4;
    color: var(--text-dim);
    background: rgba(0, 0, 0, 0.65);
    z-index: 1;
    text-align: center;
  }
  .camera-error {
    color: #fda4af;
  }
</style>
