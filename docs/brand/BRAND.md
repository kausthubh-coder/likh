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

Five rounded peach bars whose heights (0.45, 0.85, 0.55, 0.85, 0.45) trace a lowercase "m". Files in `logo/`:

- `icon.svg`: the app icon. The bars on a dusk tile (radius 116 of 512, gradient `oklch(0.25 0.016 275)` to `bg`). Window and taskbar icon, site favicon, T3 project icon.
- `mark.svg`: the bars inside a dark capsule: the listening pill, frozen. Onboarding's welcome lands on it.
- `bars.svg`: the bars alone, for 16px and below.

Wordmark: `murmur` in Geist 600, tracking -0.02em, always lowercase.

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

## Surface

The pill is solid `surface`, fully opaque. No blur, no refraction. Decided 2026-10-12:

- Real refraction on Windows means capturing the screen behind the pill every frame and running a shader on the GPU. The GPU renderer alone idles at 229 to 313 MB, against 7 MB for the software renderer (`docs/plan.md`, Measured so far). A pill you see for a few seconds at a time can't justify that.
- The OS blur (Acrylic) only rounds window corners to about 8px, so a capsule would get a blurred rectangle behind it.
- A near-black capsule reads over white pages and dark apps alike, so it never needs a tint that changes with the background.

Every pill edge has the same recipe: an inner top highlight (`white / 0.10`, 1px), an outer hairline (`black / 0.5`, 0.5px), and two shadows, a tight one (`0 1 2 black / 0.2`) and a soft one (`0 8 24 black / 0.24`). Windows (onboarding, settings) are solid `bg`.

## Type

Geist for UI, with tabular figures for anything that counts (timers, latency). Geist Mono only for key names and paths. Both SIL OFL, bundled with the app. Five sizes and nothing else:

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
- **Level bars:** 7 bars, attack 40ms, release 140ms, driven by mic RMS on a -55 to -10 dBFS scale. The middle bar shows the newest level and outer bars older ones, each a little shorter, so loud speech still reads as a shape. Frames are requested only while listening; idle draws zero frames.
- **Thinking:** bars settle into a slow travelling wave (1.2s loop). No spinner.
- **Reduced motion:** morphs become 150ms crossfades, the travelling wave becomes a gentle opacity pulse, level bars stay (they carry meaning).

## Pill states

The pill hugs what it shows. Seven bars (4px wide, 3px apart, 46px in all) sit 15px from the left edge, which centres them in the 76px listening pill. Hands-free and the countdown add one 24px circle on the right, inset 6px so it shares the capsule's centre (radius 18 − 6 = 12).

| State | Size | Look |
|---|---|---|
| `resting` | 40 x 8 | dim sliver, 24px above the taskbar, bottom centre. Can be hidden. |
| `listening` | 76 x 36 | 7 peach level bars, 4 to 20px tall |
| `hands-free` | 100 x 36 | bars + a stop button (8px square in the 24px circle). Clicking it pastes, Esc cancels |
| `thinking` | 76 x 36 | travelling wave, bars turn `text-muted` |
| `done` | back to resting | 250ms; the pasted text is the feedback, no checkmark toast |
| `time running out` | 100 x 36 | from 9:00: bars + an amber ring that empties over the last minute, seconds left inside it (`55`) |
| `error` | 360 x 52 | icon, one sentence, one pill-shaped action (36 tall, inset 8). Stays 6s or until hovered |
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

Five steps in one borderless window, 560 x 640, no title bar: drag it anywhere, Alt+F4 closes it. Each step slides 24px on `spring.default`; the back button and the progress dots stay still, and the wide dot slides to the new step. Every step has the same skeleton (art, display title, one line of body, a note line, the actions), so nothing jumps. No cards: the state is the headline.

1. **Welcome**: the flock flies in from the left and lands on the mark, then title, body and button arrive 50ms apart. "Talk. It types." · Get started
2. **Connect Codex**: murmur's bars, a dotted link, a terminal prompt for Codex. The link carries the state's icon and colour; the title is the state from the copy table above. Re-checks every 2s while visible.
3. **Microphone**: nine live bars from the default mic, newest in the middle. The device name underneath. If no mic opens: "Can't hear a microphone" with Open settings.
4. **Shortcut**: big key caps `Ctrl` `Win`. Inside the app this is a real first dictation: hold the keys, talk, let go, and the words land in a practice box.
5. **Done**: the flock spells murmur in dots (`spring.playful`, the one bounce). "Hold Ctrl + Win in any app to talk." · Finish

The opening flock plays each time the window opens; the window only opens the first time. The finish plays once. Screens on the board are rendered from the app's own views (`cargo test -p murmur brand_board -- --ignored`).
