//! Styles per app: with Clean up on "By app", the one ChatGPT pass also takes
//! the tone of the app in front when the shortcut went down. An email gets
//! full sentences and its greeting on its own line, a chat stays casual, notes
//! keep lists as lists, and code keeps its terms. Other apps get Clean up as is.
//!
//! The app is the window in front (`hotkey::foreground`): its program, and
//! for a browser its title, since Gmail and Slack also live in tabs.

use crate::hotkey::Window;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Email,
    Chat,
    Notes,
    Code,
}

/// Programs, by file name, lowercase.
const EMAIL_APPS: [&str; 5] = ["outlook.exe", "olk.exe", "thunderbird.exe", "hxoutlook.exe", "mailspring.exe"];
const CHAT_APPS: [&str; 9] = [
    "slack.exe",
    "discord.exe",
    "teams.exe",
    "ms-teams.exe",
    "whatsapp.exe",
    "telegram.exe",
    "signal.exe",
    "messenger.exe",
    "zoom.exe",
];
const NOTES_APPS: [&str; 6] =
    ["notion.exe", "obsidian.exe", "onenote.exe", "evernote.exe", "winword.exe", "notepad.exe"];
const CODE_APPS: [&str; 12] = [
    "code.exe",
    "code - insiders.exe",
    "cursor.exe",
    "windsurf.exe",
    "zed.exe",
    "devenv.exe",
    "idea64.exe",
    "pycharm64.exe",
    "webstorm64.exe",
    "rider64.exe",
    "sublime_text.exe",
    "t3 code.exe",
];
/// Words a browser tab's title carries, lowercase, for the web apps people write in.
const EMAIL_TITLES: [&str; 5] = ["gmail", "outlook", "proton mail", "fastmail", "superhuman"];
const CHAT_TITLES: [&str; 7] =
    ["slack", "discord", "whatsapp", "messenger", "telegram", "microsoft teams", "google chat"];
const NOTES_TITLES: [&str; 4] = ["notion", "google docs", "obsidian", "onenote"];

impl Style {
    pub fn name(self) -> &'static str {
        match self {
            Style::Email => "email",
            Style::Chat => "chat",
            Style::Notes => "notes",
            Style::Code => "code",
        }
    }

    /// The style for the window in front, if its app is one likh knows.
    pub fn of(window: &Window) -> Option<Style> {
        let app = window.app.to_ascii_lowercase();
        let title = window.title.to_lowercase();
        let titled = |words: &[&str]| words.iter().any(|word| title.contains(word));
        if EMAIL_APPS.contains(&app.as_str()) {
            Some(Style::Email)
        } else if CHAT_APPS.contains(&app.as_str()) {
            Some(Style::Chat)
        } else if CODE_APPS.contains(&app.as_str()) || crate::editing::is_terminal(&app) {
            Some(Style::Code)
        } else if NOTES_APPS.contains(&app.as_str()) {
            Some(Style::Notes)
        } else if titled(&EMAIL_TITLES) {
            Some(Style::Email)
        } else if titled(&CHAT_TITLES) {
            Some(Style::Chat)
        } else if titled(&NOTES_TITLES) {
            Some(Style::Notes)
        } else {
            None
        }
    }

    /// What Clean up is told on top of its usual job.
    pub fn instruction(self) -> &'static str {
        match self {
            Style::Email => {
                "It is going into an email. Write it in complete sentences with standard capitalization and \
                 punctuation. When the speaker says a greeting (\"Hi Priya\") or a sign-off (\"Thanks, Sam\"), put \
                 the greeting on its own line followed by a blank line, and the sign-off after a blank line. Never \
                 add a greeting or sign-off they didn't say."
            }
            Style::Chat => {
                "It is going into a chat message. Keep it short and casual, the way people type in chat: plain \
                 punctuation, no greeting or sign-off they didn't say, and no period after a lone sentence."
            }
            Style::Notes => {
                "It is going into notes or a document. Use complete sentences, and make anything the speaker lists \
                 into a list."
            }
            Style::Code => {
                "It is going into a code editor or a terminal, often as a prompt for an AI coding agent or a code \
                 comment. Keep technical terms, file names, commands and identifiers as the speaker means them \
                 (camelCase or snake_case when they spell it out), never turn them into prose, and add nothing \
                 that isn't a correction."
            }
        }
    }
}
