use super::*;

#[test]
fn edits_and_translations_read_as_instructions() {
    for (said, instruction) in [
        ("Make it shorter.", "Make it shorter"),
        ("make this more formal", "make this more formal"),
        ("In Hindi.", "In Hindi"),
        ("in spanish please", "in spanish please"),
        ("Translate this to French.", "Translate this to French"),
        ("Fix the grammar.", "Fix the grammar"),
        ("Rewrite it as bullet points!", "Rewrite it as bullet points"),
        ("Summarize this in two sentences.", "Summarize this in two sentences"),
        ("More casual.", "More casual"),
        ("Translate to French.", "Translate to French"),
        ("Make it sound more professional.", "Make it sound more professional"),
        ("Turn this into a list.", "Turn this into a list"),
        ("Clean it up.", "Clean it up"),
        ("Expand on this.", "Expand on this"),
        ("Shorter.", "Shorter"),
        ("As bullet points.", "As bullet points"),
    ] {
        assert_eq!(super::instruction(said).as_deref(), Some(instruction), "{said}");
    }
}

#[test]
fn ordinary_sentences_are_typed_not_treated_as_edits() {
    for said in [
        "Longer meetings mean less focus.",
        "Send the launch notes to the team by Friday.",
        "I made it to the station.",
        "In the morning we ship.",
        "In Hindi we say namaste to everyone who comes to the door.",
        "Make it shorter and then send it to Priya before the call tomorrow morning at nine.",
        "",
        // Sentences that start like an edit (Devin found these replacing a selection, 2026-10-22).
        "Expand the team next quarter.",
        "Make it happen by Friday.",
        "Fix it by Friday.",
        "Make it a priority.",
        "More people came than we planned.",
        "Translate the launch notes for the Paris team.",
        "Rewrite the intro tomorrow.",
        "Summarize the meeting for Priya.",
        "Change the date to Tuesday.",
        "In Spain we eat late.",
    ] {
        assert_eq!(super::instruction(said), None, "{said}");
    }
}

#[test]
fn terminals_never_get_a_ctrl_c() {
    for app in ["WindowsTerminal.exe", "cmd.exe", "pwsh.exe", "powershell.exe", "wezterm-gui.exe"] {
        assert!(is_terminal(app), "{app}");
    }
    for app in ["chrome.exe", "WINWORD.EXE", "slack.exe", "Code.exe"] {
        assert!(!is_terminal(app), "{app}");
    }
}

#[test]
fn chatgpt_gets_the_instruction_then_the_selection() {
    assert_eq!(request("In Hindi", "See you at nine."), "Instruction: In Hindi\n\nSelected text:\nSee you at nine.");
}

#[test]
fn a_code_editors_whole_line_copy_is_no_selection() {
    // VS Code with nothing selected copies the cursor's line, line break and all.
    assert!(is_a_copied_line("let total = items.len();\n"));
    assert!(is_a_copied_line("    return total;\r\n"));
    // What people select: part of a line, or several lines.
    assert!(!is_a_copied_line("items.len()"));
    assert!(!is_a_copied_line("fn main() {\n    run();\n}\n"));
}
