//! The language to transcribe in, when the user pins one. Auto-detect is the
//! default and the right choice for anyone who mixes languages: a pinned
//! language makes Whisper write only that one, translating the rest.
//!
//! The list is what OpenAI documents for its transcription models, plus the
//! Indian languages Whisper also knows. Codes are ISO-639-1, which Groq's
//! `language` field takes; ChatGPT's `/transcribe` takes a `language` part
//! too (docs/codex.md), in a format nobody documents.

/// A language: its ISO-639-1 code and its English name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    pub code: &'static str,
    pub name: &'static str,
}

/// Sorted by English name, the way the list shows them.
pub const ALL: [Language; 62] = [
    Language { code: "af", name: "Afrikaans" },
    Language { code: "ar", name: "Arabic" },
    Language { code: "hy", name: "Armenian" },
    Language { code: "az", name: "Azerbaijani" },
    Language { code: "be", name: "Belarusian" },
    Language { code: "bn", name: "Bengali" },
    Language { code: "bs", name: "Bosnian" },
    Language { code: "bg", name: "Bulgarian" },
    Language { code: "ca", name: "Catalan" },
    Language { code: "zh", name: "Chinese" },
    Language { code: "hr", name: "Croatian" },
    Language { code: "cs", name: "Czech" },
    Language { code: "da", name: "Danish" },
    Language { code: "nl", name: "Dutch" },
    Language { code: "en", name: "English" },
    Language { code: "et", name: "Estonian" },
    Language { code: "fi", name: "Finnish" },
    Language { code: "fr", name: "French" },
    Language { code: "gl", name: "Galician" },
    Language { code: "de", name: "German" },
    Language { code: "el", name: "Greek" },
    Language { code: "gu", name: "Gujarati" },
    Language { code: "he", name: "Hebrew" },
    Language { code: "hi", name: "Hindi" },
    Language { code: "hu", name: "Hungarian" },
    Language { code: "is", name: "Icelandic" },
    Language { code: "id", name: "Indonesian" },
    Language { code: "it", name: "Italian" },
    Language { code: "ja", name: "Japanese" },
    Language { code: "kn", name: "Kannada" },
    Language { code: "kk", name: "Kazakh" },
    Language { code: "ko", name: "Korean" },
    Language { code: "lv", name: "Latvian" },
    Language { code: "lt", name: "Lithuanian" },
    Language { code: "mk", name: "Macedonian" },
    Language { code: "ms", name: "Malay" },
    Language { code: "ml", name: "Malayalam" },
    Language { code: "mi", name: "Maori" },
    Language { code: "mr", name: "Marathi" },
    Language { code: "ne", name: "Nepali" },
    Language { code: "no", name: "Norwegian" },
    Language { code: "fa", name: "Persian" },
    Language { code: "pl", name: "Polish" },
    Language { code: "pt", name: "Portuguese" },
    Language { code: "pa", name: "Punjabi" },
    Language { code: "ro", name: "Romanian" },
    Language { code: "ru", name: "Russian" },
    Language { code: "sr", name: "Serbian" },
    Language { code: "sk", name: "Slovak" },
    Language { code: "sl", name: "Slovenian" },
    Language { code: "es", name: "Spanish" },
    Language { code: "sw", name: "Swahili" },
    Language { code: "sv", name: "Swedish" },
    Language { code: "tl", name: "Tagalog" },
    Language { code: "ta", name: "Tamil" },
    Language { code: "te", name: "Telugu" },
    Language { code: "th", name: "Thai" },
    Language { code: "tr", name: "Turkish" },
    Language { code: "uk", name: "Ukrainian" },
    Language { code: "ur", name: "Urdu" },
    Language { code: "vi", name: "Vietnamese" },
    Language { code: "cy", name: "Welsh" },
];

/// The language for a saved code. A code likh doesn't know (an older or
/// hand-edited file) is None, which means auto-detect.
pub fn find(code: &str) -> Option<Language> {
    ALL.iter().copied().find(|language| language.code == code)
}
