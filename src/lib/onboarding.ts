// Contextual onboarding state — plain localStorage flags, no backend, no deps.
//
// Design: every hint is quiet + permanently dismissible. A hint hides once the
// user has *used* the feature (used.*) OR explicitly dismissed it
// (dismissed.*). The guided tour shows once on a brand-new install and never
// again after any dismissal. All storage access is try/catch so private-mode
// or blocked storage never breaks the app.

export interface OnboardingState {
  /** First-run welcome finished or skipped; Help can always reopen it. */
  welcomeDismissed: boolean;
  /** Guided tour permanently dismissed (finished, skipped, or X). */
  tourDismissed: boolean;
  /** Per-feature "has actually used it" flags — auto-hide the matching hint. */
  used: Record<string, boolean>;
  /** Per-hint explicit dismissals (× on the hint itself). */
  dismissed: Record<string, boolean>;
}

const KEY = "makrstudio.onboarding.v1";

const EMPTY: OnboardingState = { welcomeDismissed: false, tourDismissed: false, used: {}, dismissed: {} };

export function loadOnboarding(): OnboardingState {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { ...EMPTY, used: {}, dismissed: {} };
    const parsed = JSON.parse(raw) as Partial<OnboardingState>;
    return {
      welcomeDismissed: parsed.welcomeDismissed === true,
      tourDismissed: parsed.tourDismissed === true,
      used: { ...(parsed.used ?? {}) },
      dismissed: { ...(parsed.dismissed ?? {}) },
    };
  } catch {
    return { ...EMPTY, used: {}, dismissed: {} };
  }
}

function save(state: OnboardingState): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(state));
  } catch {
    // Storage unavailable — hints simply show again next launch. Never throw.
  }
}

/** Welcome finished / skipped. Does not reset hints or tour progress. */
export function dismissWelcome(state: OnboardingState): OnboardingState {
  const next = { ...state, welcomeDismissed: true };
  save(next);
  return next;
}

/** Record that the user has used a feature; its hint never shows again. */
export function markUsed(state: OnboardingState, feature: string): OnboardingState {
  if (state.used[feature]) return state;
  const next: OnboardingState = {
    ...state,
    used: { ...state.used, [feature]: true },
  };
  save(next);
  return next;
}

/** Explicit × dismissal of one hint line. */
export function dismissHint(state: OnboardingState, hint: string): OnboardingState {
  if (state.dismissed[hint]) return state;
  const next: OnboardingState = {
    ...state,
    dismissed: { ...state.dismissed, [hint]: true },
  };
  save(next);
  return next;
}

/** Tour finished / skipped / closed — never auto-show again. */
export function dismissTour(state: OnboardingState): OnboardingState {
  if (state.tourDismissed) return state;
  const next: OnboardingState = { ...state, tourDismissed: true };
  save(next);
  return next;
}

/** Re-open the tour on demand (Help entry point). Does not clear hints. */
export function resetTourDismissal(state: OnboardingState): OnboardingState {
  if (!state.tourDismissed) return state;
  const next: OnboardingState = { ...state, tourDismissed: false };
  save(next);
  return next;
}

/** Show hint while the feature is unused AND the hint wasn't dismissed. */
export function showHint(state: OnboardingState, key: string): boolean {
  return !state.used[key] && !state.dismissed[key];
}
