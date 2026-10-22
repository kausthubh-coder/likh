# likh brand

The rules every screen follows. `brand.html` next to this file shows them moving. When code and this file disagree, fix one of them in the same change.

## Story

likh (लिख्) is the Sanskrit root for *write*. You talk; it writes, right where you are typing. The palette is dusk, the motion is a flock of dots finding its place like ink on a page: one surface that changes shape, never a pile of popups.

Name: always lowercase, `likh`. Tagline: **Talk. It types.**

## Feel

| Surface | Who, how often | Feel |
|---|---|---|
| Pill (overlay) | People who type all day, dozens of times a day | liquid, quiet, instant |
| Onboarding | Everyone, once | spacious, springy, expressive. Spend the novelty budget here |
| Settings, history, usage | A few times a week | calm, compact, quiet |

The pill is the product. It must never steal focus, never block the cursor and never animate while idle.

## Logo

Five rounded peach bars whose heights (0.45, 0.85, 0.55, 0.85, 0.45) are a voice level, mid-word. Files in `logo/`:

- `icon.svg`: the app icon. The bars on a dusk tile (radius 116 of 512, gradient `oklch(0.25 0.016 275)` to `bg`). Window and taskbar icon, site favicon, T3 project icon.
- `mark.svg`: the bars inside a dark capsule: the listening pill, frozen. Onboarding's welcome lands on it.
- `bars.svg`: the bars alone, for 16px and below.

Wordmark: `likh` in Geist 600, tracking -0.02em, always lowercase.

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
| `spring.playful` | 0.5s, 0.3 | onboarding finish, the first successful dictation, and a key cap landing when you pick a new shortcut. Rare moments only |
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
| `stopping after quiet` | 100 x 36 | hands-free after 1 s of quiet: a peach ring around the stop button empties over the chosen quiet; the last 3 s show the seconds (`2`) instead of the square. Talking again clears it. Peach, not amber: this is the normal end |
| `thinking` | 76 x 36 | travelling wave, bars turn `text-muted` |
| `done` | back to resting | 250ms; the pasted text is the feedback, no checkmark toast |
| `time running out` | 100 x 36 | from 29:00: bars + an amber ring that empties over the last minute, seconds left inside it (`55`) |
| `error` | 200 to 360 x 52 | red icon, one sentence (two lines at most). Stays 6 s, longer while the pointer is on it |
| `problem` | 200 to 360 x 52 | an error with its fix: one pill-shaped button on the right (36 tall, inset 8) that opens it and dismisses the pill. "No sound from Microphone Array. Is it muted?" · Pick mic (opens likh at the Microphone row). A transcription that may pass (no connection, a busy server, a sign-in to refresh) keeps its audio: "Can't reach ChatGPT. Check your connection and try again." · Retry (sends it again; paste last does too). Another app holding the shortcut at launch: "Another app has Ctrl + Space, so likh can't hear it." · Change keys (opens Dictation, listening for new keys) |
| `note` | hugs its words x 36 (52 for two lines) | nothing broke, nothing happened: three grey bars lying flat, one sentence, no sound. "No voice heard". Stays 3 s. Also "Clean up is off until 17 Nov: ChatGPT's allowance ran out.", once, on the dictation that found it |
| `mic silent` | hugs its words x 36 | while recording, after 2 s of digital silence: the note's look, "Microphone Array is silent. Is it muted?". Stays until sound arrives (the bars come back) or the dictation ends |
| `cancelled` | back to resting | bars fall flat, `exit` timing |
| action `listening` | 100 x 36 (124 hands-free) | the command key (the action key in code): a white four-point spark in a 24px `elevated` circle on the left (inset 6), then the bars. Every recording state of an action carries it; dictation never does |
| action `running` | hugs its words x 36 | the spark, the action's name, three small `text-muted` bars on the thinking wave |
| action `finished` | hugs its words x 36 | a `success` check in the slot, the action's result or name on one line. Stays 2 s |
| action `confirm` | hugs its words x 36 | the spark with a peach ring that empties over 5 s, "Run Deploy preview?", the action key and Esc as `elevated` caps (12px). Peach: running it is the normal answer |
| action `review` | the `problem` look | "deploy.toml isn't approved yet, so it won't run." · Review (opens Commands, on the command key's half). A failed action: "Run tests failed: …" · Details |

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
| not installed | Codex isn't installed. likh uses your ChatGPT account through Codex, free on any plan. Install it, sign in once. | Get Codex · Check again |
| signed out | Codex is installed but not signed in. Open Codex (or run `codex login`) and sign in. | Check again |
| connected | Connected through Codex · k•••@gmail.com · ChatGPT Free. Note: "Dictation is free on every plan. Clean up, a bonus, can run out on Free." | Continue |
| expired | Your Codex sign-in expired. Open Codex once to refresh it, then try again. | Open Codex |
| rejected | ChatGPT turned down the sign-in. Sign in to Codex again. | Open Codex |
| offline | Can't reach ChatGPT. Check your connection and try again. | none |

The other engines' copy (onboarding and settings, `onboarding::engine_copy`):

| State | Message | Action |
|---|---|---|
| Groq, no key | Add a free Groq key. Make one at console.groq.com, then paste it here. | Get a free key · Save key |
| Groq, key saved | Groq is ready. Your key is saved on this computer, in its credential store. | Continue |
| Groq, key refused | Groq turned down that key. Copy it again from Groq. | Save key |

## Settings window

Calm and compact (Feel, above). Seven tabs, each with one job: Dictation, Transcription, Commands, Dictionary, History, App, Diagnostics. Commands' icon is the pill's spark, outlined. Each has a drawn 16px icon in the sidebar, peach when it's the open tab. At the sidebar's foot: the version, and next to it one update button (check, spinning while checking, peach once an update is ready to restart into).

- **Panels**: settings sit in rounded `surface` panels (radius 12, a 1px `edge`), one setting per row with a hairline between rows: the name on the left, the control on the right, a line under the name only when the name can't say it alone. A row that does something lights with the `hover` veil, rounded only where it meets the panel's corners. Every tab fits in the window without scrolling, except a long History and Commands' phrase groups; there is no scrollbar, and the edges fade.
- **The shortcut** (Dictation): its key caps beside a small keyboard (15 units wide, keys on `elevated`) with the shortcut's keys lit peach. `Change` empties the caps into a dashed slot; each key you press lights on the keyboard and flies off it into the next slot (`spring.playful`), and the saved shortcut pulses once, left to right (1.08, 360ms, 45ms apart). It moves only then.
- **Segmented pick** (mouse button, mode, silence stop, the onboarding engine pick): a `surface` track, radius 10, inset 3; the picked segment lifts to `elevated` with a 1px shadow, radius 10 − 3 = 7. The rest are words in `text-muted` until pointed at. The mode explains only the picked one, under its name.
- **Clean up** (Transcription): a segmented pick, Off · Everywhere · By app. Its line says what it does ("A tone for each app: email, chat, notes, code." for By app) and that it can run out on Free.
- **Engines** (Transcription): one panel per engine, each always showing its own setup under its name (the Codex sign-in, or the Groq key field), so picking one moves nothing. The picked panel takes a 1.5px peach edge, a peach radio and a peach glyph; Groq's `Save key` turns peach only while Groq is picked. A dot says ready (`success`) or not.
- **Commands**: a segmented pick on top, "While dictating" (the phrase groups below) and "With the command key". The command key's half: the command key (a mono chip, Change, Turn off) and its mouse button; "Your commands" with Open folder and the one peach New command; one row per action file, its phrases as capsules and what it does in Geist Mono `text-subtle`, a switch on the right. A row that needs you swaps the switch for Review and says why beside a dot: `warning` for a script waiting for approval, `danger` for a broken file (`file:line message`) or a failed last run. Review shows the change since approval as mono lines (added on a `success` wash, removed on a `danger` wash), then Runs, Reads and Result, then Not now and the peach Approve. New command is one panel of labelled rows and a peach Save; "When I say" takes several phrases, comma-separated. Under the list: "Say it your way: when no phrase matches, ChatGPT picks the closest command." In code and in the folder a command is still an action (`actions/`, `likh actions`, `ctl action-start`).
- **Command phrases**: each group's name, then what it does (always, on or off), then each phrase in a small capsule in quotes (the pill's shape, at rest) with what it does in Geist Mono `text-subtle`, left off when every phrase in the group does the same thing. A group's phrases show only while it's on.

## Onboarding

Five steps in one borderless window, 560 x 640, no title bar: drag it anywhere, Alt+F4 closes it. Each step slides 24px on `spring.default`; the back button and the progress dots stay still, and the wide dot slides to the new step. Every step has the same skeleton (art, display title, one line of body, a note line, the actions), so nothing jumps. No cards: the state is the headline.

1. **Welcome**: the flock flies in from the left and lands on the mark, then title, body and button arrive 50ms apart. "Talk. It types." "Free dictation with any ChatGPT account. Hold a key, say it, let go." · Get started
2. **Connect**: a segmented pick on top (ChatGPT · Groq · Windows; Windows only on Windows), then likh's bars, a dotted link and the engine's glyph: a terminal prompt for ChatGPT through Codex, a bolt for Groq, four panes for Windows. The link carries the state's icon and colour; the title is the state (the Codex copy table above, or the engine copy below). A slot under the art holds the Groq key field and stays reserved for the other engines, so the pick never moves when you switch. Re-checks every 2s while the picked engine (Codex or Windows) isn't ready.
3. **Microphone**: nine live bars from the default mic, newest in the middle. The device name underneath. If no mic opens: "Can't hear a microphone" with Open settings.
4. **Shortcut**: big key caps `Ctrl` `Space`. Inside the app this is a real first dictation: hold the keys, talk, let go, and the words land in a practice box. If another app has the keys, the step says so first: "Another app has these keys" · Skip · Change shortcut.
5. **Done**: the flock spells likh in dots (`spring.playful`, the one bounce), above three keys as small mono caps with what they do: the shortcut ("Hold to talk, let go to paste"), Esc ("Cancel") and paste last ("Paste the last one again", left out when it's off). "likh waits in your tray and starts with Windows." · Finish turns on Start with Windows (on by default), opens settings and saves setup completion. Closing setup before Finish leaves it unfinished.

The opening flock plays each time the window opens; the window only opens the first time. The finish plays once. Screens on the board are rendered from the app's own views (`cargo test -p likh brand_board -- --ignored`).
