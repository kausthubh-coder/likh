use super::*;

fn window(app: &str, title: &str) -> Window {
    Window { id: 1, app: app.into(), title: title.into() }
}

#[test]
fn desktop_apps_get_their_style_by_program() {
    for (app, style) in [
        ("OUTLOOK.EXE", Style::Email),
        ("olk.exe", Style::Email),
        ("slack.exe", Style::Chat),
        ("ms-teams.exe", Style::Chat),
        ("Code.exe", Style::Code),
        ("Cursor.exe", Style::Code),
        ("WindowsTerminal.exe", Style::Code),
        ("Notion.exe", Style::Notes),
        ("WINWORD.EXE", Style::Notes),
    ] {
        assert_eq!(Style::of(&window(app, "")), Some(style), "{app}");
    }
}

#[test]
fn web_apps_in_a_browser_get_theirs_by_title() {
    for (title, style) in [
        ("Inbox (3) - k•••@gmail.com - Gmail - Google Chrome", Style::Email),
        ("general - Acme - Slack - Microsoft Edge", Style::Chat),
        ("(2) WhatsApp - Google Chrome", Style::Chat),
        ("Launch plan - Google Docs - Google Chrome", Style::Notes),
    ] {
        assert_eq!(Style::of(&window("chrome.exe", title)), Some(style), "{title}");
    }
}

#[test]
fn any_other_app_gets_clean_up_as_it_is() {
    assert_eq!(Style::of(&window("chrome.exe", "Hacker News - Google Chrome")), None);
    assert_eq!(Style::of(&window("explorer.exe", "Downloads")), None);
}

#[test]
fn mac_apps_get_their_style_by_bundle_id() {
    for (app, name, style) in [
        ("com.apple.mail", "Mail", Style::Email),
        ("com.tinyspeck.slackmacgap", "Slack", Style::Chat),
        ("com.microsoft.VSCode", "Code", Style::Code),
        ("com.jetbrains.intellij", "IntelliJ IDEA", Style::Code),
        ("com.apple.Terminal", "Terminal", Style::Code),
        ("com.apple.Notes", "Notes", Style::Notes),
    ] {
        assert_eq!(Style::of(&window(app, name)), Some(style), "{app}");
    }
}
