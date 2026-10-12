# murmur brand

The rules every screen follows. `brand.html` next to this file shows them moving. When code and this file disagree, fix one of them in the same change.

## Story

A murmuration is thousands of starlings moving as one shape at dusk. murmur turns a stream of speech into text that lands where you are typing. The palette is dusk, the motion is the flock: one surface that changes shape, never a pile of popups.

Name: always lowercase, `murmur`. Tagline: **Talk. It types.**

## Feel

| Surface | Who, how often | Feel |
|---|---|---|
| Pill (overlay) | People who type all day, dozens of times a day | liquid, quiet, instant |
| Onboarding | Everyone, once | spacious, springy, expressive. Spend the novelty budget here |
| Settings, history, usage | A few times a week | calm, compact, quiet |

The pill is the product. It must never steal focus, never block the cursor and never animate while idle.

## Logo

A dark glass capsule holding five rounded bars whose heights (0.45, 0.85, 0.55, 0.85, 0.45) trace a lowercase "m". It is the listening pill, frozen. Wordmark: `murmur` in Geist 600, tracking -0.02em. Minimum mark size 16px; at 16px drop the capsule and keep the bars.

## Color (OKLCH, dark-first)

Neutrals do almost all the work. Peach is for "you are being heard" and the one primary action per screen. Green, amber and red only when they mean something.

| Token | Value | Job |
|---|---|---|
| `bg` | `oklch(0.155 0.012 275)` | window canvas, dusk ink |
| `surface` | `oklch(0.19 0.014 275)` | cards, inputs |
| `elevated` | `oklch(0.225 0.016 275)` | menus, popovers |
| `text` | `oklch(0.945 0.006 275)` | primary text |
| `text-muted` | `oklch(0.72 0.012 275)` | secondary text |
| `text-subtle` | `oklch(0.56 0.014 275)` | hints, disabled |
| `accent` | `oklch(0.78 0.11 45)` | dusk peach: live level bars, primary button |
| `on-accent` | `oklch(0.22 0.03 45)` | text on accent |
| `success` | `oklch(0.78 0.13 155)` | connected, done |
| `warning` | `oklch(0.84 0.14 85)` | time running out |
| `danger` | `oklch(0.68 0.17 25)` | errors |

Elevation on dark comes from lightness (+0.035 per level), plus a faint top highlight. Light theme comes later, designed separately, not inverted.

## Glass

Glass is only for surfaces that float above other apps: the pill and its error/notice expansions. Windows (onboarding, settings) are solid `bg` with Mica allowed behind the title area. Never glass on glass.

Rungs, picked at runtime:
1. **Solid**: `surface` at 96% opacity. Used when the OS asks for less transparency, or the user picks "Solid surfaces".
2. **Frosted (default)**: OS blur behind the window (Acrylic on Windows, system blur on macOS), tinted with a top-to-bottom gradient from `oklch(0.24 0.014 275 / 0.72)` to `oklch(0.13 0.012 275 / 0.78)`, so it reads over a white page or a black video. Lighter tints looked like a grey smudge over white pages.
3. **Refraction**: real lensing at the rim. Only ships if the Windows benchmark says it is cheap enough to leave on (see `docs/plan.md`). macOS uses the native Liquid Glass view.

Every glass edge has two lines: an inner top highlight (`white / 0.18`, 1px) and an outer hairline (`black / 0.45`, 0.5px), plus one soft shadow (`0 8 24 black / 0.35`). Never animate blur radius; animate the shape's size and opacity.

## Type

Geist for UI, Geist Mono for shortcuts, timers and numbers (tabular). Both SIL OFL, bundled with the app. Five sizes and nothing else:

| Token | Size / line | Weight | Job |
|---|---|---|---|
| `caption` | 12 / 16 | 500 | key caps, pill labels, metadata |
| `small` | 13 / 18 | 400, 500 | secondary text, buttons |
| `body` | 15 / 22 | 400 | body, inputs |
| `title` | 20 / 26 | 600 | window and section titles, -0.01em |
| `display` | 34 / 38 | 600 | onboarding headlines only, -0.02em |

## Space and shape

- Spacing scale: 4, 8, 12, 16, 24, 32, 48, 64. Gaps inside a group are half the gap between groups or less.
- Radius: windows 16, cards 12, controls 10, the pill always half its height. Nested radius = outer minus padding.
- Hit areas at least 32px, even when the icon is 16px.

## Motion

| Token | Spring (duration, bounce) | Use |
|---|---|---|
| `spring.fast` | 0.25s, 0 | pill state changes, toggles |
| `spring.default` | 0.4s, 0 | window and onboarding step transitions |
| `spring.playful` | 0.5s, 0.3 | onboarding finish and the first successful dictation only |
| `fade` | 150ms, `cubic-bezier(0.23, 1, 0.32, 1)` | opacity and color (never springs) |
| `exit` | 120ms ease-in | anything leaving |

Spring conversion (mass 1): `stiffness = (2π / duration)²`, `damping = 4π(1 − bounce) / duration`. Springs keep their velocity when retargeted, so a state change mid-motion turns around instead of restarting.

- **One liquid surface.** The pill grows out of its resting sliver, morphs into thinking, and expands into an error message. Errors are never a separate toast while the pill is visible.
- **Level bars:** 7 bars, attack 40ms, release 140ms, driven by mic RMS. Frames are requested only while listening; idle draws zero frames.
- **Thinking:** bars settle into a slow travelling wave (1.2s loop). No spinner.
- **Reduced motion:** morphs become 150ms crossfades, the travelling wave becomes a gentle opacity pulse, level bars stay (they carry meaning).

## Pill states

| State | Size | Look |
|---|---|---|
| `resting` | 40 x 8 | dim glass sliver, 24px above the taskbar, bottom centre. Can be hidden. |
| `listening` | 148 x 40 | 7 peach level bars |
| `hands-free` | 172 x 40 | bars + a small lock, Esc hint on hover |
| `thinking` | 148 x 40 | travelling wave, bars turn `text-muted` |
| `done` | back to resting | 250ms; the pasted text is the feedback, no checkmark toast |
| `warning` | 196 x 40 | bars + mono countdown `0:55 left`, amber |
| `error` | up to 360 x 56 | icon, one sentence, one action. Stays 6s or until hovered |
| `cancelled` | back to resting | bars fall flat, `exit` timing |

## Sound

Quiet cues that confirm what you already see. Synthesized, no audio files, gain 0.04 to 0.08, one switch to turn them all off.

- start: two rising sine notes (660 → 880 Hz), 90ms
- stop: one 1200 Hz tick, 16ms
- error: falling 440 → 330 Hz, 160ms
- Nothing on paste, hover or typing.

## Words

Plain, short, second person. Buttons say what they do. Errors say what happened and how to fix it.

Codex sign-in copy (used by onboarding and errors):

| State | Message | Action |
|---|---|---|
| checking | Looking for Codex… | none |
| not installed | murmur transcribes with your ChatGPT plan through Codex. Install Codex, sign in once, then come back. | Get Codex · Check again |
| signed out | Codex is installed but not signed in. Open Codex (or run `codex login`) and sign in. | Check again |
| connected | Connected through Codex as k•••@gmail.com | Continue |
| expired | Your Codex sign-in expired. Open Codex once to refresh it, then try again. | Open Codex |
| rejected | ChatGPT turned down the sign-in. Sign in to Codex again. | Open Codex |
| offline | Can't reach ChatGPT. Your recording is saved, retry it from History. | Retry |

## Onboarding

Six steps, one window (560 x 640), each step slides on `spring.default`. Progress is a row of six dots.

1. **Welcome**: murmuration of dots that flocks into the logo. "Talk. It types." · Get started
2. **Connect Codex**: the sign-in states above, checked live every 2s while visible.
3. **Microphone**: permission, live peach level bars, device picker. "Say something. The bars should move."
4. **Shortcut**: big key caps `Ctrl` `Win`. "Hold to talk. Let go to paste." Press it to test; change it.
5. **Try it**: a practice box. "Hold Ctrl + Win and say: *Remind me to send the deck tomorrow at nine.*" Shows the result appear.
6. **Done**: the flock spells murmur (`spring.playful`), then "murmur lives in your tray." · Finish

The finish animation plays once, never on a restart.
