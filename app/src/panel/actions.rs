//! The Actions tab: the action key and its mouse button, every action in the
//! folder as its file says (with a switch each), what's broken and where,
//! Review and Approve for scripts, and a form that writes simple actions.
//! It reads the folder each time the tab opens; it never watches it.

use super::{Line, Panel, Picking, group, hint, label, link, named, small_button, tone_color};
use crate::actions::trust::{self, Trust};
use crate::actions::{Action, Catalog, Context, Draft, DraftKind, Output, Step};
use crate::hotkey::{self, MouseTrigger};
use crate::onboarding::Tone;
use crate::shared::theme::{self, Palette, size, space};
use crate::shared::ui::{field_style, pill_fill, said, segmented, switch};
use iced::widget::{Space, column, container, row, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Task, keyboard};
use std::collections::BTreeMap;

/// The label column of the New action form.
const FORM_LABEL: f32 = 92.0;
/// A changed script shows at most this many lines in Review.
const DIFF_LINES: usize = 200;

#[derive(Debug, Clone)]
pub enum Message {
    /// Pick new keys for the action key.
    ChangeKey,
    /// The action key on its default keys, or off.
    Key(Option<crate::hotkey::Binding>),
    Mouse(MouseTrigger),
    OpenFolder,
    /// An action's file, by path: open it in the app that edits it.
    OpenFile(String),
    Toggle(String, bool),
    Review(String),
    Approve(String),
    Back,
    New,
    Name(String),
    Say(String),
    Kind(DraftKind),
    Value(String),
    Selection(bool),
    Output(Output),
    Save,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    List,
    Review(String),
    New,
}

/// The folder as last read, and the view open on it.
#[derive(Default)]
pub struct State {
    catalog: Catalog,
    trust: BTreeMap<String, Trust>,
    pub view: View,
    form: Draft,
    say: String,
}

impl Panel {
    /// Reads the folder again, and where each action's scripts stand.
    pub(super) fn load_actions(&mut self) {
        self.actions.view = View::List;
        let Some(dir) = self.files.actions.clone() else { return };
        if let Err(reason) = crate::actions::prepare(&dir) {
            self.notice = Some((format!("Couldn't set up the actions folder: {reason}"), Tone::Warning));
        }
        self.actions.catalog = Catalog::load(&dir);
        self.actions.trust = self
            .actions
            .catalog
            .actions
            .iter()
            .map(|action| {
                let state = match &self.files.approved {
                    Some(store) => trust::state_in(store, action),
                    None => Trust::New,
                };
                (action.id.clone(), state)
            })
            .collect();
    }

    pub(super) fn update_actions(&mut self, message: Message) -> Task<super::Message> {
        match message {
            Message::ChangeKey => {
                self.stop_capturing();
                hotkey::pause(true);
                self.capturing = Some(keyboard::Modifiers::empty());
                self.picking = Picking::ActionKey;
                self.notice = None;
            }
            Message::Key(keys) => match hotkey::set_action_key(keys) {
                Ok(()) => self.change(|s| s.action_key = keys),
                Err(reason) => self.notice = Some((reason, Tone::Danger)),
            },
            Message::Mouse(trigger) => {
                // One button, one job: the hotkey thread takes it from dictation too.
                self.change(|s| {
                    s.action_mouse = trigger;
                    if trigger != MouseTrigger::Off && s.mouse == trigger {
                        s.mouse = MouseTrigger::Off;
                    }
                });
                if let Err(reason) = hotkey::set_action_mouse(trigger) {
                    self.notice = Some((format!("The mouse button won't work yet: {reason}"), Tone::Danger));
                }
            }
            Message::OpenFolder => {
                if let Some(dir) = &self.files.actions {
                    crate::shared::open_url(&dir.to_string_lossy());
                }
            }
            Message::OpenFile(path) => crate::shared::open_url(&path),
            Message::Toggle(id, on) => {
                if let Some(action) = self.actions.catalog.find(&id)
                    && let Err(reason) = crate::actions::set_enabled(&action.file, on)
                {
                    self.notice = Some((format!("Couldn't change {id}.toml: {reason}"), Tone::Danger));
                }
                self.load_actions();
            }
            Message::Review(id) => self.actions.view = View::Review(id),
            Message::Approve(id) => {
                let approved = match (self.actions.catalog.find(&id), &self.files.approved) {
                    (Some(action), Some(store)) => trust::approve_in(store, action),
                    _ => Err("it's gone from the folder".into()),
                };
                self.load_actions();
                self.notice = Some(match approved {
                    Ok(()) => ("Approved. It runs from now on, until its files change.".into(), Tone::Success),
                    Err(reason) => (format!("Couldn't approve it: {reason}"), Tone::Danger),
                });
            }
            Message::Back => self.actions.view = View::List,
            Message::New => {
                self.actions.form = Draft::default();
                self.actions.say.clear();
                self.actions.view = View::New;
                self.notice = None;
            }
            Message::Name(name) => self.actions.form.name = name,
            Message::Say(phrase) => self.actions.say = phrase,
            Message::Kind(kind) => {
                self.actions.form.kind = kind;
                // Only ChatGPT's answer has something to paste back.
                if kind != DraftKind::Ask {
                    self.actions.form.output = Output::None;
                }
            }
            Message::Value(value) => self.actions.form.value = value,
            Message::Selection(on) => {
                self.actions.form.input = if on { vec![Context::Selection] } else { Vec::new() };
                if !on && self.actions.form.output == Output::Replace {
                    self.actions.form.output = Output::Paste;
                }
            }
            Message::Output(output) => {
                self.actions.form.output = output;
                if output == Output::Replace && !self.actions.form.input.contains(&Context::Selection) {
                    self.actions.form.input = vec![Context::Selection];
                }
            }
            Message::Save => {
                let mut draft = self.actions.form.clone();
                draft.say = vec![self.actions.say.trim().to_owned()];
                let saved = match &self.files.actions {
                    Some(dir) => draft.save(dir),
                    None => Err("there is no actions folder".into()),
                };
                match saved {
                    Ok(path) => {
                        self.load_actions();
                        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        self.notice = Some((format!("Saved {name}. Hold the action key and say it."), Tone::Success));
                    }
                    Err(why) => self.notice = Some((format!("Not saved: {why}"), Tone::Danger)),
                }
            }
        }
        Task::none()
    }

    pub(super) fn actions_tab(&self, p: &Palette) -> Element<'_, super::Message> {
        match &self.actions.view {
            View::List => self.actions_list(p),
            View::Review(id) => match self.actions.catalog.find(id) {
                Some(action) => self.review(p, action),
                None => self.actions_list(p),
            },
            View::New => self.new_action(p),
        }
    }

    fn actions_list(&self, p: &Palette) -> Element<'_, super::Message> {
        let chosen = self.chosen();
        let picking = self.capturing.is_some() && self.picking == Picking::ActionKey;
        let (what, control): (&str, Element<'_, super::Message>) = match chosen.action_key {
            _ if picking => ("Press the new keys.", link(p, "Cancel", super::Message::KeepShortcut)),
            Some(keys) => (
                if hotkey::action_key_taken() {
                    "Another app has these keys. Change them."
                } else {
                    "Hold it and say an action."
                },
                row![
                    container(text(String::from(keys)).size(size::SMALL).font(theme::MONO).color(p.text))
                        .padding([space::S1, space::S3])
                        .style({
                            let fill = p.elevated;
                            move |_| pill_fill(fill)
                        }),
                    link(p, "Change", act(Message::ChangeKey)),
                    link(p, "Turn off", act(Message::Key(None))),
                ]
                .spacing(space::S3)
                .align_y(Alignment::Center)
                .into(),
            ),
            None => ("Off", link(p, "Turn on", act(Message::Key(Some(hotkey::ACTION_KEY))))),
        };
        let mouse_options = MouseTrigger::ALL.map(|trigger| (trigger, trigger.name()));
        let keys = group(
            p,
            vec![
                Line::plain(named(p, "Action key", Some(what)), control),
                Line::plain(
                    named(p, "Mouse button", Some("Works like the action key.")),
                    segmented(&mouse_options, chosen.action_mouse, |t| act(Message::Mouse(t))),
                ),
            ],
        );
        let top = row![
            label(p, "Your actions"),
            Space::new().width(Length::Fill),
            link(p, "Open folder", act(Message::OpenFolder)),
            small_button(p, "New action", act(Message::New), true),
        ]
        .spacing(space::S4)
        .padding(iced::Padding { right: space::S4, ..iced::Padding::ZERO })
        .align_y(Alignment::Center);
        let catalog = &self.actions.catalog;
        let mut rows: Vec<Line<'_>> = catalog.actions.iter().map(|action| self.action_row(p, action)).collect();
        rows.extend(catalog.problems.iter().map(|problem| {
            let path = self
                .files
                .actions
                .as_ref()
                .map(|dir| dir.join(&problem.file).to_string_lossy().into_owned())
                .unwrap_or_default();
            let at = problem.line.map_or(String::new(), |line| format!(":{line}"));
            let left = column![
                text(problem.file.clone()).size(size::BODY).font(theme::UI_MEDIUM),
                status(p, Tone::Danger, format!("{}{at} {}", problem.file, problem.message), true),
            ]
            .spacing(space::S1);
            Line::plain(left.into(), link(p, "Open file", act(Message::OpenFile(path))))
        }));
        let list: Element<'_, super::Message> = if rows.is_empty() {
            hint(p, "No actions yet. Make one with New action, or ask an agent to write one into the folder.")
        } else {
            group(p, rows)
        };
        column![
            keys,
            column![top, list].spacing(space::S2),
            hint(p, "Each action is a file in the folder. Agents can write them; a program runs only after you approve it here."),
        ]
        .spacing(space::S4)
        .into()
    }

    fn action_row<'a>(&'a self, p: &Palette, action: &'a Action) -> Line<'a> {
        let trust = self.actions.trust.get(&action.id).copied().unwrap_or(Trust::New);
        let under: Element<'a, super::Message> = match (trust, crate::actions::runner::last_error(&action.id)) {
            (Trust::New, _) => status(p, Tone::Warning, "Runs a program. Approve it to use it.".into(), false),
            (Trust::Changed, _) => {
                status(p, Tone::Warning, "Changed since you approved it. Approve it to run.".into(), false)
            }
            (Trust::Approved, Some(error)) => status(p, Tone::Danger, format!("Last run: {error}"), false),
            (Trust::Approved, None) => {
                let mut chips = row(action.say.iter().map(|phrase| said(phrase))).spacing(space::S2);
                chips =
                    chips.push(text(what_it_does(action)).size(size::CAPTION).font(theme::MONO).color(p.text_subtle));
                chips.wrap().vertical_spacing(space::S2).into()
            }
        };
        let left = column![text(action.name.as_str()).size(size::BODY).font(theme::UI_MEDIUM), under]
            .spacing(space::S2)
            .width(Length::Fill);
        match trust {
            Trust::Approved => Line::plain(left.into(), switch(action.enabled))
                .press(act(Message::Toggle(action.id.clone(), !action.enabled))),
            Trust::New | Trust::Changed => {
                Line::plain(left.into(), small_button(p, "Review", act(Message::Review(action.id.clone())), false))
            }
        }
    }

    fn review<'a>(&'a self, p: &Palette, action: &'a Action) -> Element<'a, super::Message> {
        let changes = match &self.files.approved {
            Some(store) => trust::changes_in(store, action),
            None => Vec::new(),
        };
        let fresh = self.actions.trust.get(&action.id) == Some(&Trust::New);
        let title = if fresh {
            format!("{} runs a program you haven't approved yet", action.name)
        } else {
            format!("{} changed after you approved it", action.name)
        };
        let (added, removed) = changes.iter().fold((0, 0), |(a, r), change| {
            let (more, fewer) = change.counts();
            (a + more, r + fewer)
        });
        let sub = format!("{} added, {} removed. Read it before it can run.", lines(added), lines(removed));
        let mut body = column![
            text(title).size(size::BODY).font(theme::UI_MEDIUM),
            text(sub).size(size::SMALL).color(p.text_muted),
        ]
        .spacing(space::S1);
        for change in &changes {
            body = body.push(Space::new().height(space::S2));
            body = body.push(text(change.file.clone()).size(size::CAPTION).font(theme::MONO).color(p.text_muted));
            body = body.push(diff(p, change));
        }
        let runs: Vec<String> = action
            .steps
            .iter()
            .filter_map(|step| match step {
                Step::Run(argv) => Some(argv.join(" ")),
                _ => None,
            })
            .collect();
        let reads = if action.input.is_empty() {
            "What you said".to_string()
        } else {
            format!("What you said, and {}", action.input.iter().map(|c| c.slot()).collect::<Vec<_>>().join(", "))
        };
        let facts = column![
            fact(p, "Runs", runs.join("\n"), true),
            fact(p, "Reads", reads, false),
            fact(p, "Result", sentence(action.output.name()), false),
        ]
        .spacing(space::S1);
        body = body.push(Space::new().height(space::S2)).push(facts);
        let file = action.file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let path = format!("actions{}{file}", std::path::MAIN_SEPARATOR);
        let foot = row![
            text(path).size(size::CAPTION).font(theme::MONO).color(p.text_subtle).width(Length::Fill),
            link(p, "Not now", act(Message::Back)),
            small_button(p, "Approve", act(Message::Approve(action.id.clone())), true),
        ]
        .spacing(space::S4)
        .padding(iced::Padding { left: space::S4, right: space::S4, ..iced::Padding::ZERO })
        .align_y(Alignment::Center);
        column![link(p, "\u{2039} All actions", act(Message::Back)), group(p, vec![Line::custom(body.into())]), foot,]
            .spacing(space::S3)
            .into()
    }

    fn new_action(&self, p: &Palette) -> Element<'_, super::Message> {
        let form = &self.actions.form;
        let field = |placeholder: &str, value: &str, on: fn(String) -> Message| {
            text_input(placeholder, value)
                .on_input(move |typed| act(on(typed)))
                .size(size::SMALL)
                .padding([space::S2, space::S3])
                .style(|_, status| field_style(status))
        };
        let (value_label, value_hint) = match form.kind {
            DraftKind::Paste => ("Text", "Best,\\nPriya"),
            DraftKind::Open => ("Open", "https://github.com, or spotify:search:{text}"),
            DraftKind::Keys => ("Keys", "ctrl+shift+t, or play_pause"),
            DraftKind::Ask => ("Prompt", "Rewrite this to be {style}: {selection}"),
        };
        let kinds = DraftKind::ALL.map(|kind| (kind, kind.name()));
        let outputs = [
            (Output::Replace, "Replace selection"),
            (Output::Paste, "Paste"),
            (Output::Notify, "Show in pill"),
            (Output::None, "Nothing"),
        ];
        let reads_selection = form.input.contains(&Context::Selection);
        let labelled = |words: &'static str, control: Element<'static, super::Message>| -> Line<'static> {
            Line::custom(
                row![
                    text(words).size(size::SMALL).font(theme::UI_MEDIUM).color(palette_muted()).width(FORM_LABEL),
                    control
                ]
                .spacing(space::S3)
                .align_y(Alignment::Center)
                .into(),
            )
        };
        let say = column![
            field("make this {style}", &self.actions.say, Message::Say),
            text("A {slot} at the end catches the words you say there, like {text}.")
                .size(size::CAPTION)
                .color(p.text_muted),
        ]
        .spacing(space::S1);
        let rows = vec![
            labelled("Name", field("Make this formal", &form.name, Message::Name).into()),
            labelled("When I say", say.into()),
            labelled("Do", segmented(&kinds, form.kind, |k| act(Message::Kind(k)))),
            labelled(value_label, field(value_hint, &form.value, Message::Value).into()),
            labelled(
                "Reads",
                row![switch(reads_selection), text("The selection, as {selection}").size(size::SMALL).color(p.text),]
                    .spacing(space::S3)
                    .align_y(Alignment::Center)
                    .into(),
            )
            .press(act(Message::Selection(!reads_selection))),
            labelled("Result", segmented(&outputs, form.output, |o| act(Message::Output(o)))),
        ];
        let file = match &self.files.actions {
            Some(dir) => {
                let path = form.file_in(dir);
                format!("saves {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
            }
            None => String::new(),
        };
        let foot = row![
            text(file).size(size::CAPTION).font(theme::MONO).color(p.text_subtle).width(Length::Fill),
            link(p, "Cancel", act(Message::Back)),
            small_button(p, "Save", act(Message::Save), true),
        ]
        .spacing(space::S4)
        .padding(iced::Padding { left: space::S4, right: space::S4, ..iced::Padding::ZERO })
        .align_y(Alignment::Center);
        column![group(p, rows), foot].spacing(space::S3).into()
    }
}

fn act(message: Message) -> super::Message {
    super::Message::Action(message)
}

fn palette_muted() -> iced::Color {
    theme::palette().text_muted
}

/// "show in the pill" as a value on its own line: "Show in the pill".
fn sentence(words: &str) -> String {
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

fn lines(n: usize) -> String {
    if n == 1 { "1 line".into() } else { format!("{n} lines") }
}

/// What an action does, in a few mono words: its steps, then where the result goes.
fn what_it_does(action: &Action) -> String {
    let steps = match action.steps.as_slice() {
        [only] => only.describe(),
        many => many
            .iter()
            .map(|step| match step {
                Step::Paste(_) => "paste",
                Step::Keys(_) => "keys",
                Step::Open(_) => "open",
                Step::Wait(_) => "wait",
                Step::Ask(_) => "ask",
                Step::Run(_) => "run",
            })
            .collect::<Vec<_>>()
            .join(", "),
    };
    match action.output {
        Output::Paste | Output::Replace => format!("{steps}, then {}", action.output.name()),
        Output::Notify | Output::None => steps,
    }
}

/// A dot in the tone's colour, then words: why a row needs you.
fn status<'a>(p: &Palette, tone: Tone, words: String, mono: bool) -> Element<'a, super::Message> {
    let tint = tone_color(p, tone);
    let words = text(words).size(size::SMALL).color(if mono { p.text_muted } else { p.text });
    let words = if mono { words.font(theme::MONO).size(size::CAPTION) } else { words };
    row![container(Space::new()).width(8).height(8).style(move |_| pill_fill(tint)), words]
        .spacing(space::S2)
        .align_y(Alignment::Center)
        .into()
}

/// One labelled fact under Review: Runs, Reads, Result.
fn fact<'a>(p: &Palette, name: &'static str, value: String, mono: bool) -> Element<'a, super::Message> {
    let value = text(value).size(size::SMALL).color(p.text);
    let value = if mono { value.font(theme::MONO).size(size::CAPTION) } else { value };
    row![text(name).size(size::SMALL).font(theme::UI_MEDIUM).color(p.text_muted).width(64), value]
        .spacing(space::S3)
        .into()
}

/// A file's change as lines: added on a green wash, removed on a red one, the rest quiet.
fn diff<'a>(p: &Palette, change: &trust::Change) -> Element<'a, super::Message> {
    let (bg, ink, quiet) = (p.bg, p.text, p.text_muted);
    let added = iced::Color { a: 0.14, ..p.success };
    let removed = iced::Color { a: 0.16, ..p.danger };
    let lines = change.lines.iter().take(DIFF_LINES).map(|line| {
        let (words, wash, color) = match line {
            trust::Line::Same(words) => (format!("  {words}"), None, quiet),
            trust::Line::Added(words) => (format!("+ {words}"), Some(added), ink),
            trust::Line::Removed(words) => (format!("- {words}"), Some(removed), quiet),
        };
        container(text(words).size(size::CAPTION).font(theme::MONO).color(color))
            .width(Length::Fill)
            .padding([1.0, space::S3])
            .style(move |_| iced::widget::container::Style {
                background: wash.map(Background::Color),
                ..iced::widget::container::Style::default()
            })
            .into()
    });
    container(column(lines))
        .width(Length::Fill)
        .padding([space::S2, 0.0])
        .style(move |_| iced::widget::container::Style {
            background: Some(Background::Color(bg)),
            border: Border { radius: 8.0.into(), ..Border::default() },
            ..iced::widget::container::Style::default()
        })
        .into()
}
