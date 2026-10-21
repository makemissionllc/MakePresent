# Design Guide for Coding Agents (Android + Desktop)

> **Read this before you build or change any UI.**
> Goal: my apps should feel clear, calm, trustworthy, and native to the platform they run on.
> The *thinking* comes from Apple's design principles (Purpose, Agency, Responsibility, Familiarity, Flexibility, Simplicity, Craft, Delight). The *look and behavior* come from each platform's own conventions: **Material Design 3 on Android**, and **the host OS conventions on desktop**.

---

## 0. How to use this file

- Use the **principles** (Section 2) to decide *what* to build and *how it should behave*.
- Use the **platform sections** (Sections 3–5) to decide *how it looks and which components to use*.
- **Do not copy Apple's visual style.** Do not make an Android app look like iOS. Familiarity means following the conventions of the platform the person is on.
- **Prefer built-in and standard components.** Do not build custom versions of things the framework already gives you.
- If a rule here conflicts with a direct instruction from me, follow me, and tell me the trade-off in one line.
- Simple beats clever. Never add features just to apply a principle.
- When two principles pull against each other, choose what best helps the person using the app, and say why in one sentence.
- **If you are unsure about an API, a component name, or a version, check the current official docs.** Do not guess.

---

## 1. The big idea

**Design is making something with intention.**
Every feature asks the user for their time, attention, and trust. Do not waste them. Choosing what to build often means choosing what *not* to include.

Before writing UI code, ask:
1. What is this screen or feature for?
2. Who is it for, and what are they trying to do?
3. Is anything here not helping with that?

If you cannot answer #1 in one sentence, stop and ask me.

---

## 2. The eight principles

### 2.1 Purpose: *Make something meaningful*
Every screen, button, and feature needs a reason to exist.

**Do**
- Focus on the one or two things the app does best. Make those excellent.
- Put the main task front and center.
- Ask "what is this for?" about each element.

**Avoid**
- Features added "just because."
- Home screens with many equal-weight options.
- Copying another app's layout without knowing why it works.

### 2.2 Agency: *Let people do things their own way*
The user is in control. The app helps; it never blocks.

**Do**
- Get people to the content or task quickly. Keep onboarding short or skippable.
- Let people explore in any order.
- Make actions **reversible**: Undo (snackbar on Android, Ctrl/Cmd+Z on desktop), trash or "Recently deleted", drafts, autosave.
- Confirm **only** before something destructive or hard to undo. Prefer *undo* over *"Are you sure?"* when possible.

**Avoid**
- Forced tutorials or forced sign-up before the user sees value.
- Confirmation dialogs for harmless actions (people learn to click through).
- Losing the user's work on a crash, rotation, window close, or process death.

### 2.3 Responsibility: *Act in people's best interest*
Earn trust through privacy, safety, and honesty.

**Do**
- Ask for permissions **at the moment they are needed**, not at launch. Explain *why* in plain words first.
- Collect only the data a feature truly needs. Say what you collect and why.
- Think about misuse: *How could this be misused? Who could be hurt? How do I prevent it?*
- For **AI features**: assume the model can be wrong or unsafe. Add safeguards (preview, confirm, label AI content, easy undo). For safety-critical things (health, allergies, money, legal), never rely on the model alone. If the risk outweighs the value, remove the feature.

**Avoid**
- Permission pop-ups on first launch with no context.
- Dark patterns (fake urgency, hidden cancel buttons, guilt-trip wording).
- Secrets or API keys hard-coded in the app.
- Vague privacy wording.

### 2.4 Familiarity: *Build on what people know*
People already know how their device works. Use that.

**Do**
- Use **standard components and patterns for the platform** (Section 4 for Android, Section 5 for desktop).
- Use **standard icons with their normal meaning**. Trash = delete. Share = share. Back = back. Never reuse them for something else.
- **Be consistent.** Things that look the same must behave the same. Keep controls in the same place across screens.
- Use the system's patterns for dialogs, menus, feedback, and choices.

**Avoid**
- Custom controls that replace standard ones without a strong reason.
- Changing the meaning of a familiar icon or gesture.
- Moving the same action to different places on different screens.

### 2.5 Flexibility: *Adapt to diverse contexts and needs*
People use apps on different devices, in different places, with different abilities.

**Do**
- **Respect each platform's strengths.** Phone: quick, touch, one hand. Tablet and foldable: more room, side-by-side panes. Desktop: precision, keyboard, many windows, deep workflows.
- Support **many input types**: touch, keyboard, mouse/trackpad, stylus, and assistive tech.
- Design **accessibility in from the start** (Section 6).
- Offer **personalization** when one layout can't fit all (reorder, hide, or choose what shows).
- Support different languages, text sizes, and light/dark theme.

**Avoid**
- One phone layout stretched onto a big window.
- Assuming perfect vision, hearing, motor control, or fast internet.
- Hard-coded English text or fixed pixel sizes.

### 2.6 Simplicity: *Be clear and direct*
Simple does **not** mean minimal. Every element earns its place.

**Do**
- Use **plain language**. Short labels. No jargon.
- Reduce steps. Fewer taps and clicks to finish a task is better.
- Build clear **hierarchy**: the most important thing should be the most obvious, using size, weight, spacing, and contrast.
- Make it obvious: *What should I look at? What can I interact with? How?*
- Sometimes simple means **adding context** (for example, showing time left next to a play/pause button).
- Turn complex data into a chart or a short summary.

**Avoid**
- Hiding key features in a mystery menu just to look "clean".
- Walls of text, long forms, and too many choices at once.
- Decoration that does not help understanding.

### 2.7 Craft: *Care about every detail*
Quality shows. Small details tell users whether to trust the app.

**Do**
- Use good typography, theme-aware colors, crisp icons, and smooth, responsive animation.
- Give **instant feedback** to every tap and click.
- Keep scrolling smooth. Align things carefully. Test rotation, window resizing, big text, and small screens.
- Write precise copy, including error messages.
- Prototype, test, refine, repeat. Keep up with new platform features.

**Avoid**
- Misaligned icons, janky scrolling, layouts that break when resized or rotated.
- Buttons that lag with no feedback.
- Placeholder text, "Lorem ipsum", or leftover template code in shipped UI.

### 2.8 Delight: *Make it human*
Delight is the **result** of the other seven done well. It is not confetti.

**Do**
- Decide what the app should feel like (calm, energized, confident, playful) and let that guide tone, motion, and sound.
- Add small touches of character in key moments: first success, empty states, error messages.
- Keep delight **in service of the task**.

**Avoid**
- Gimmicks added at the end.
- Animations that slow people down or repeat every time.
- Jokes in serious moments (lost data, money, health).

---

## 3. Platform rule: follow the platform

| If the app runs on... | Follow... |
|---|---|
| Android | **Material Design 3** and Android's own patterns (Jetpack Compose first) |
| Windows | Windows conventions (Fluent-style, standard title bar, Ctrl-based shortcuts) |
| macOS | macOS conventions (menu bar at top, Cmd-based shortcuts, standard window controls) |
| Linux | The user's desktop environment conventions (GNOME/KDE), Ctrl-based shortcuts |
| Many platforms with one codebase | One shared design language, but **respect each OS's basics**: shortcuts, window behavior, file dialogs, notifications, system theme |

If I haven't said which UI framework to use, **ask me once, or pick the most standard choice for the platform and tell me**. Default assumptions:
- **Android:** Kotlin + Jetpack Compose + Material 3.
- **Desktop:** Compose Multiplatform for Desktop if the project already uses Kotlin; otherwise follow the framework already in the project. Do not switch frameworks on your own.

---

## 4. Android (Material 3 + Jetpack Compose)

### 4.1 Theming (use the theme, never raw values)
- Wrap the app in `MaterialTheme` and read values from it: `MaterialTheme.colorScheme`, `MaterialTheme.typography`, `MaterialTheme.shapes`.
- **Never hard-code colors, font sizes, or corner radii inside screens.** Define them once in the theme.
- **Dynamic color** (Material You, Android 12+): support it by default, with a fallback static color scheme for older devices. Let the user's wallpaper colors shape the app unless I say the brand colors must stay fixed.
- **Always support light and dark theme.** Test both. Follow the system setting.
- Pick **one primary/accent color family** for interactive elements. Don't use ten.
- **Never use color alone to carry meaning.** Add an icon, label, or shape.
- Contrast: about **4.5:1** for normal text, **3:1** for large text and key UI.

### 4.2 Material 3 Expressive
Material 3 Expressive is Google's newer, more expressive direction (bolder shapes, spring motion, emphasized type, refreshed components). Use it **when the project's Material 3 library version supports it** and when it fits the app's feel. Check the current Compose Material 3 release notes for exact APIs and versions before using them. If unsure, use standard Material 3, which is always a safe choice.

### 4.3 Layout
- **Edge-to-edge:** draw behind the system bars, and handle insets correctly (status bar, navigation bar, display cutouts, keyboard/IME). Content must never be hidden under system bars.
- Use a consistent spacing scale based on **8dp** (4, 8, 12, 16, 24, 32). Typical screen side margin is 16dp. Define spacing values once.
- **Adaptive layouts, not phone-only.** Use window size classes (compact, medium, expanded). Support phones, foldables, tablets, ChromeOS, and desktop windows. Use adaptive scaffolds and navigation components (for example the Material 3 adaptive libraries) instead of hand-built breakpoints.
- Support portrait and landscape, split-screen, and resizable windows. Never assume screen size.
- Group related things; use white space on purpose.

### 4.4 Navigation
- **Compact width (phones):** bottom navigation bar for 3–5 top-level destinations.
- **Medium/expanded width:** navigation rail or permanent/modal drawer. Switch automatically (the adaptive navigation suite does this).
- Always label destinations with short text **and** an icon.
- **Back must work and behave as expected**, including the system back gesture. Support **predictive back**.
- Use **bottom sheets** and **dialogs** for focused, temporary tasks. Always give a clear way out.
- Keep the user's place: preserve scroll position, selection, and typed input across rotation, theme changes, and process death (`rememberSaveable`, `SavedStateHandle`).
- Do not nest navigation bars or mix many navigation styles on one screen.

### 4.5 Typography
- Use the **Material type scale** (`displayLarge` … `labelSmall`, via `MaterialTheme.typography`). Do not make up sizes.
- **Text sizes in `sp`** so they follow the user's font-size setting. Layouts must survive the largest font scale (test at 200%). Let text wrap and layouts reflow instead of clipping.
- Build hierarchy with weight and size, not with many font families. One family is enough.
- Custom fonts only if I ask, and they must still scale.

### 4.6 Components
| Need | Use |
|---|---|
| Primary action on a screen | `FloatingActionButton` or a filled `Button`. One primary action per screen. |
| Secondary actions | `OutlinedButton`, `TextButton` |
| Top of screen | `TopAppBar` (small, medium, or large) with a title and back/up button when needed |
| Lists | `LazyColumn` with stable `key`s, list items, dividers only if needed. Swipe-to-dismiss with **Undo snackbar**. |
| Short feedback | `Snackbar` (with Undo if reversible). Not dialogs. |
| Important decision or destructive confirm | `AlertDialog` with verb buttons ("Delete", "Cancel") |
| Settings | Simple list of rows with switches; changes apply immediately |
| Input | `TextField` / `OutlinedTextField` with a label, supporting text for errors, right keyboard type, and IME action |
| Search | Material `SearchBar` pattern |
| Empty state | Short explanation + one clear action |
| Loading | Progress indicator, or placeholder/skeleton content. Keep layout stable so it doesn't jump. |
| Errors | Say what happened and what to do next. Never blame the user. Never show raw exceptions. |

### 4.7 Touch, feedback, and motion
- **Touch targets at least 48 × 48 dp.** Space them so they don't overlap.
- Give immediate feedback: ripple, pressed state, haptic where meaningful (`HapticFeedback`). Don't overuse haptics.
- Use Material motion: short, natural, spring or standard easing. Motion should **explain** (where something came from or went), not decorate.
- **Never block input during animations.**
- Respect the system's reduced-animation setting. Replace big movements with fades.
- Show progress for anything slower than about a second.

### 4.8 Android-specific behavior
- **Permissions:** request at point of use, explain first, handle "denied" gracefully (the feature degrades, and there's a way to open Settings). Prefer privacy-friendly options (the system photo picker, approximate location) over broad permissions.
- **Notifications:** only useful ones; use channels so users can control them. Never use them for marketing without clear consent.
- Handle config changes and process death without losing state.
- Don't do heavy work on the main thread (coroutines, `Dispatchers.IO`, WorkManager for background work).
- Be careful with battery and data use.

### 4.9 Compose habits for agents
- Keep composables **small, stateless where possible, and reusable**. Hoist state up.
- `modifier: Modifier = Modifier` as the first optional parameter on reusable composables.
- Use `@Preview` for multiple states: light, dark, large font, empty, error, loading, and different window sizes.
- Strings go in `strings.xml` / `stringResource()`, never hard-coded in composables.
- Colors, type, and shapes come from the theme. Spacing and sizes come from one shared tokens file.
- Use `remember`, `derivedStateOf`, and stable keys properly to avoid wasted recomposition.
- Prefer modern, non-deprecated APIs. If you're not sure an API exists, check the docs.
- Keep logic in ViewModels / state holders, not in composables.

---

## 5. Desktop apps

Desktop is not "a big phone." People use it with a **keyboard and mouse**, with **many windows**, for **longer, deeper work**. Design for that.

### 5.1 Windows and layout
- **Resizable windows** that adapt gracefully. Set a sensible **minimum size**. Never let content overlap or vanish when resized.
- **Remember window size, position, and state** (maximized, panel layout) between sessions.
- Use **more space well**: sidebars, multi-pane layouts, inspectors, split views. Don't just stretch a phone layout.
- Provide a **standard title bar and window controls** (minimize, maximize, close) unless there's a strong reason for a custom one. If custom, it must still support dragging, snapping, and OS shortcuts.
- Support **high-DPI / multiple monitors** and sharp rendering.
- Consider multi-window support for apps where people compare or multitask.

### 5.2 Keyboard (this is required, not optional)
- **Everything must be doable by keyboard.** Logical **Tab order**, visible **focus indicators**, Enter/Space to activate, Esc to cancel or close dialogs, arrow keys in lists and menus.
- Add **keyboard shortcuts** for common actions, and show them in menus and tooltips.
- Use **platform-standard shortcuts**: Ctrl on Windows/Linux, Cmd on macOS. Examples: copy/paste/cut, undo/redo, save, open, find, new, close window, quit, select all, settings (Ctrl+, / Cmd+,).
- Never override standard system shortcuts.
- A **command palette** (Ctrl/Cmd+K or Ctrl/Cmd+Shift+P) is a great addition for power-user apps.

### 5.3 Mouse and pointer
- Show **hover states** on interactive elements. Use the right **cursor** (pointer/hand for links and buttons, text cursor for text, resize cursors).
- Provide **right-click context menus** with relevant actions.
- Provide **tooltips** for icon-only buttons (short, with the shortcut).
- Support **drag and drop** where it's natural (files, reordering, moving items).
- Support **double-click**, **multi-select** (Shift/Ctrl/Cmd+click), and **scroll wheel / trackpad gestures** as people expect.
- Never make a feature available *only* through hover. Touch and keyboard users need a way too.

### 5.4 Menus, toolbars, and structure
- Use a **menu bar** (File, Edit, View, Window, Help) for apps with many features. On macOS the menu bar is at the top of the screen and is expected. Keep actions in the standard menus where people look for them.
- **Toolbar** holds the most common actions. Less common actions go in menus.
- **Settings/Preferences** in a standard place with standard shortcut. Group settings clearly. Apply changes immediately where possible.
- Include a **Help/About** entry with the version.
- Use **sidebars** for navigation between sections, **tabs** for multiple open documents or views.

### 5.5 Density and sizing
- Desktop UIs can be **denser** than mobile because the pointer is precise. Typical comfortable control height is about 32–40 px. Keep touch-sized targets (44–48) if the app also supports touch.
- Keep text readable (generally 13–16 px for body text). Respect OS **text scaling / display scaling**.
- Don't make everything tiny just to fit more in. Hierarchy and spacing still matter.

### 5.6 Look and feel
- **Follow the OS light/dark setting** and the system accent color where possible.
- Use the platform's **native file dialogs, notifications, and system tray / menu bar integration** instead of custom ones.
- Use one consistent icon set (for example Material Symbols), sized consistently, with clear meaning.
- Keep the look consistent across screens. Use design tokens (colors, spacing, radius, type) defined in one place.
- It's fine for a cross-platform app to look the same on all OSes, **as long as basic behaviors (shortcuts, window controls, dialogs) match each OS**.

### 5.7 Work, data, and reliability
- **Never freeze the UI.** Run slow work in the background with a **progress indicator** and a **Cancel** button when it makes sense.
- **Autosave** and recover from crashes. Warn before closing with unsaved changes (and offer Save / Don't Save / Cancel).
- Support **Undo/Redo** for any user edit. Make it multi-level where it matters.
- Handle **offline** and slow-network states gracefully.
- Make installs and **updates** simple and transparent. Tell the user what changed, and never force a restart in the middle of work.
- Log errors for debugging, but show the user a clear message and next step.
- Be careful with **permissions and file access**. Ask for access to files and folders only when needed. Explain why.

### 5.7a Framework notes
- **Compose Multiplatform (Desktop):** reuse Material 3 theming and tokens from Android. Add desktop-specific handling for window state, keyboard shortcuts, hover (`onPointerEvent`/hoverable), context menus, tooltips, and menu bar.
- **Web-based desktop (Electron, Tauri, etc.):** use CSS variables for design tokens, `prefers-color-scheme` for dark mode, `:focus-visible` for keyboard focus, `rem` units for scalable text, semantic HTML, and ARIA labels. Don't make it feel like a website in a box: add native menus, shortcuts, and window behavior.
- **Other frameworks (Qt, GTK, .NET, Flutter desktop):** use the framework's standard widgets and theming first, and follow the same principles above.

---

## 6. Accessibility (required on every platform)

A feature is not done until it passes this list.

- **Screen readers:** every interactive element has a clear name and role.
  - Android: TalkBack. Set `contentDescription` for icons and images (and `null` for decorative ones). Use `Modifier.semantics`, merge related elements, set correct roles and states.
  - Desktop: expose accessible names and roles using the framework's accessibility APIs (UI Automation on Windows, NSAccessibility on macOS, AT-SPI on Linux, or ARIA for web-based).
- **Text scaling:** support the user's font/display scaling without clipping or overlap.
- **Touch targets:** at least 48 dp on Android; sensibly sized on desktop (and larger if touch is supported).
- **Contrast:** 4.5:1 for normal text, 3:1 for large text and key UI.
- **Color:** never the only signal.
- **Keyboard:** fully usable by keyboard with clear focus indicators (Section 5.2). On Android also support external keyboards and D-pad navigation for tablets and Chromebooks.
- **Motion:** respect reduced-motion settings. Don't flash content.
- **Gestures:** every gesture has a visible, tappable/clickable alternative.
- **Captions and transcripts** for audio and video.
- **Localization:** use string resources, support right-to-left layouts, avoid text baked into images, and avoid layouts that assume English text length.

---

## 7. Privacy, safety, and AI (all platforms)

- Ask for a permission **right before** the feature needs it, and explain the benefit.
- If permission is denied, the app must **still work as well as possible** and offer a way to change it later.
- Collect the **minimum** data. Keep data on-device when you can.
- Store secrets in the platform's secure storage (Android Keystore / encrypted storage; OS keychain/credential store on desktop). Never hard-code keys.
- Be clear about what is shared and with whom.
- **AI features:** label AI content; add preview/confirm/undo; never rely on a model alone for safety-critical things; explain what data is sent; provide a fallback when the model is unavailable or slow; let people correct it.

---

## 8. Writing UI text

- **Short, plain, human.** "Couldn't save. Check your connection and try again." Not "Error 500: Operation failed."
- Buttons start with a **verb**: "Add", "Share", "Delete photo".
- Be consistent with capitalization (Material uses sentence case for most UI text). Match the platform.
- Don't over-apologize or say "please" everywhere. Be calm and direct.
- Use the user's words, not internal terms.
- Error messages say **what happened** and **what to do next**.

---

## 9. Self-review checklist (before saying "done")

**Purpose & Simplicity**
- [ ] I can say what this screen is for in one sentence.
- [ ] The most important thing is the most obvious thing.
- [ ] I removed anything that doesn't help.
- [ ] Text is short and plain.

**Agency**
- [ ] The user can go back, cancel, or skip.
- [ ] Undo exists for user actions. Confirmation is only for destructive things.
- [ ] User work survives rotation, resize, crash, and close.

**Responsibility**
- [ ] Permissions are requested at the right time, with a reason.
- [ ] Only needed data is collected.
- [ ] Misuse and harm are considered (especially for AI).

**Familiarity**
- [ ] I used standard components and icons for this platform.
- [ ] Similar things look and behave the same.
- [ ] Platform shortcuts and back behavior work as expected.

**Flexibility & Accessibility**
- [ ] Light and dark theme both work.
- [ ] Large text / display scaling doesn't break the layout.
- [ ] Works at different window/screen sizes.
- [ ] Screen reader names and order are correct.
- [ ] Touch targets are big enough; desktop is fully keyboard-usable.
- [ ] Color is never the only signal; contrast is sufficient.
- [ ] Reduced motion is respected.

**Craft**
- [ ] Spacing, alignment, and type are consistent and come from the theme/tokens.
- [ ] Taps and clicks respond instantly; loading shows progress.
- [ ] No placeholder text, debug UI, or hard-coded strings.
- [ ] UI never freezes.

**Delight**
- [ ] It feels the way the app is meant to feel.
- [ ] No "delight" is getting in the way of the task.

---

## 10. Anti-patterns: never do these

1. Permission requests at launch with no context.
2. Forcing sign-up or a long tutorial before showing value.
3. Custom controls that replace working standard ones.
4. Making an Android or desktop app look like iOS.
5. Reusing familiar icons (trash, share, back) for different actions.
6. Hard-coded colors, font sizes, spacing, or strings inside screens.
7. Text or layouts that break at large font/display scaling.
8. Meaning shown by color alone.
9. Tiny tap targets on touch devices (under 48 dp on Android).
10. Desktop features that only work with a mouse (no keyboard path).
11. Hover-only features.
12. Overriding standard system shortcuts.
13. A phone layout stretched across a large window or tablet.
14. Confirmation dialogs for harmless actions.
15. Destructive actions with no confirmation and no undo.
16. Animations that block input or ignore reduced-motion settings.
17. Hiding primary features inside menus to look "minimal".
18. Dark patterns (fake urgency, hidden opt-outs, guilt wording).
19. Raw errors or stack traces shown to users.
20. Freezing the UI on slow work.
21. Losing unsaved work.
22. Relying on an AI model alone for safety-critical decisions.
23. Hard-coded secrets or API keys.

---

## 11. When you're unsure

1. Reread the matching principle in Section 2.
2. Choose the **most standard, native, simple** option for the platform.
3. Check the current official docs instead of guessing (Material 3, Compose, or the desktop framework's docs).
4. If the choice would change how the app feels and you still can't decide, **ask me one short question**.
5. In your summary, tell me in a line or two which design decisions you made and why.

---

## 12. References

- Apple Design Principles (the source of the eight principles): https://developer.apple.com/design/principles/
- Material Design 3: https://m3.material.io/
- Android design and Compose docs: https://developer.android.com/design and https://developer.android.com/compose
- Adaptive layouts (Android): https://developer.android.com/develop/ui/compose/layouts/adaptive
- Android accessibility: https://developer.android.com/guide/topics/ui/accessibility
- Compose Multiplatform (Desktop): https://www.jetbrains.com/compose-multiplatform/
- WCAG contrast and accessibility basics: https://www.w3.org/WAI/standards-guidelines/wcag/
