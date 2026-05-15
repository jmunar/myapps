pub struct Translations {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub nav_notes: &'static str,
    pub new_note: &'static str,
    pub empty: &'static str,
    pub delete: &'static str,
    pub delete_confirm: &'static str,
    pub back: &'static str,
    pub untitled: &'static str,
    pub updated: &'static str,
    pub pinned: &'static str,
    pub pin: &'static str,
    pub unpin: &'static str,
    pub dictate: &'static str,
    pub dictating: &'static str,
    pub transcribing: &'static str,
    pub search_placeholder: &'static str,
}

pub const EN: Translations = Translations {
    title: "Notes",
    subtitle: "Markdown-based note-taking",
    nav_notes: "Notes",
    new_note: "+ New note",
    empty: "No notes yet. Create one to get started.",
    delete: "Delete",
    delete_confirm: "Delete this note?",
    back: "Back to notes",
    untitled: "Untitled",
    updated: "Updated",
    pinned: "Pinned",
    pin: "Pin",
    unpin: "Unpin",
    dictate: "Dictate",
    dictating: "Recording…",
    transcribing: "Transcribing…",
    search_placeholder: "Search notes…",
};

pub const ES: Translations = Translations {
    title: "Notas",
    subtitle: "Toma de notas en Markdown",
    nav_notes: "Notas",
    new_note: "+ Nueva nota",
    empty: "Sin notas aún. Crea una para empezar.",
    delete: "Eliminar",
    delete_confirm: "¿Eliminar esta nota?",
    back: "Volver a notas",
    untitled: "Sin título",
    updated: "Actualizada",
    pinned: "Fijada",
    pin: "Fijar",
    unpin: "Desfijar",
    dictate: "Dictar",
    dictating: "Grabando…",
    transcribing: "Transcribiendo…",
    search_placeholder: "Buscar notas…",
};

pub fn t(lang: myapps_core::i18n::Lang) -> &'static Translations {
    match lang {
        myapps_core::i18n::Lang::En => &EN,
        myapps_core::i18n::Lang::Es => &ES,
    }
}
