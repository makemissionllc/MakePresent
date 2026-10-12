/** Own async event registrations, including ones that resolve after teardown. */
export function createListenerScope() {
  let active = true;
  const listeners = new Set<() => void>();
  function release(unlisten: () => void): void {
    try { unlisten(); }
    catch (error) { console.error("Could not release event listener", error); }
  }
  return {
    get active() { return active; },
    add(registration: Promise<() => void>): void {
      void registration.then((unlisten) => {
        if (active) listeners.add(unlisten);
        else release(unlisten);
      }).catch((error) => {
        if (active) console.error("Could not register event listener", error);
      });
    },
    dispose(): void {
      active = false;
      for (const unlisten of listeners) release(unlisten);
      listeners.clear();
    },
  };
}
