# likh actions

Hold the action key (Ctrl+Shift+Space unless you changed it, or the mouse
button you picked), say what to do, and likh does it. Each `*.toml` file in
this folder is one action. likh reads the folder again every time you press
the key, so a saved file works on the next press. likh rewrites this README
and `actions.schema.json`; every other file here is yours.

Files ending in `.example.toml` are ignored. Rename one to use it.

## Check your work without speaking

```sh
likh actions check                         # every file, and what's wrong with it, with line numbers
likh actions test "new thread fix the bug" # which action that runs, its slots and steps; runs nothing
```

Both work while likh is closed. `LIKH_HOME` picks the folder (`LIKH_HOME/actions`).

## A file

```toml
#:schema ./actions.schema.json
name = "New T3 thread"                       # required
description = "Start a T3 Code thread"       # for ChatGPT, when no phrase matched
say = ["new thread {text}"]                  # required: one or more phrases
output = "none"                              # paste | replace | notify (default) | none

[[do]]
open = "t3code://new?prompt={text}"
```

| Key | What it does |
|---|---|
| `say` | Phrases. Case, punctuation and apostrophes don't matter; the words and their order do. |
| `lists` | `[lists.site]` then `github = "https://github.com"`: a `{site}` slot matches "github" and becomes the link. |
| `input` | Anything of `"selection"`, `"clipboard"`, `"app"`, `"title"`. Only what's listed is read. |
| `output` | What happens to the last step's result. `replace` needs `input = ["selection"]`. |
| `confirm` | `true` asks in the pill first: press the action key again to run it, Esc or 5 s to cancel. |
| `router` | `false` keeps ChatGPT from ever picking this action. |
| `enabled` | `false` turns it off (the switch on likh's Actions tab writes this). |
| `timeout` | Seconds a `run` step may take, 1 to 600 (default 30). |

## Slots

A `{name}` in a phrase is a slot. If the file has `[lists.name]`, the slot
matches one of that list's names. Otherwise it catches the rest of what was
said, so it must come last, and there can be only one.

Steps can use: every slot of the action's phrases, `{said}` (all of it),
`{result}` (the last `ask` or `run` answer), and `{selection}`,
`{clipboard}`, `{app}`, `{title}` when they're in `input`. Write `{{` and `}}`
for literal braces.

## Steps

Each `[[do]]` has exactly one of these, and they run in order.

| Step | Example | Result |
|---|---|---|
| `paste` | `paste = "Best,\nPriya"` | unchanged |
| `keys` | `keys = "ctrl+shift+t"`, `keys = "play_pause"` | unchanged |
| `open` | `open = "spotify:search:{text}"` | unchanged |
| `wait` | `wait = 300` (ms, at most 10000) | unchanged |
| `ask` | `ask = "Translate to French: {selection}"` | ChatGPT's answer |
| `run` | `run = ["powershell", "-NoProfile", "-File", "deploy.ps1"]` | what it printed |

Keys: `a`-`z`, `0`-`9`, `f1`-`f12`, `enter`, `tab`, `esc`, `space`,
`backspace`, `delete`, `up`, `down`, `left`, `right`, `home`, `end`,
`pageup`, `pagedown`, `play_pause`, `next_track`, `previous_track`,
`volume_up`, `volume_down`, `mute`, with `ctrl+`, `alt+`, `shift+`, `win+`.

In `open`, what you said is URL-encoded; a list's value goes in as written.

## Programs (`run`)

- No shell, and no slots in the arguments. The program starts in this folder
  and gets `LIKH_TEXT` (the slot that caught the rest), `LIKH_SAID`,
  `LIKH_SLOT_<NAME>` for each slot, `LIKH_SELECTION`, `LIKH_CLIPBOARD`,
  `LIKH_APP`, `LIKH_TITLE` and `LIKH_RESULT` when there are any, and all of
  them as one JSON object on stdin.
- What it prints is the result. A non-zero exit fails the action, and its last
  line on stderr is what the pill says.
- **It never runs until you approve it** on likh's Actions tab, and again
  after any change to the action's file or a script next to it that it runs.
  Agents can write and edit actions; only a person can approve them.
- If ChatGPT picked the action (no phrase matched), likh asks in the pill
  before running a program.

## When no phrase matches

ChatGPT gets the name, description and phrases of every action with
`router = true` (the default) and picks one, or none. Unsure means nothing
runs. Matching a phrase never leaves the computer.

## More examples

```toml
name = "Spotify search"
say = ["spotify {text}", "spotify play {text}"]
output = "none"
[[do]]
open = "spotify:search:{text}"
```

```toml
name = "Next song"
say = ["next song", "skip this song"]
output = "none"
[[do]]
keys = "next_track"
```

```toml
name = "Open a site"
say = ["open {site}"]
output = "none"
[lists.site]
github = "https://github.com"
"likh docs" = "https://getlikh.vercel.app"
[[do]]
open = "{site}"
```

An app from the Start menu opens as `shell:AppsFolder\<id>`; `Get-StartApps`
in PowerShell lists the ids. Use single quotes so TOML keeps the backslash.

```toml
name = "Open an app"
say = ["open {target}"]
output = "none"
[lists.target]
zen = 'shell:AppsFolder\F0DC299D809B9700'
terminal = 'shell:AppsFolder\Microsoft.WindowsTerminal_8wekyb3d8bbwe!App'
[[do]]
open = "{target}"
```

A T3 Code thread in one of your projects: `repo` is the project's GitHub
repo, which is how T3 Code finds it ("new thread in krida fix the login").

```toml
name = "T3 thread in a project"
say = ["new thread in {project} {text}", "new thread in {project} then {text}"]
output = "none"
[lists.project]
krida = "your-name/krida"
[[do]]
open = "t3code://new?repo={project}&prompt={text}"
```
