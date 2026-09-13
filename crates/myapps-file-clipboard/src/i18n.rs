//! FileClipboard translations. Both `EN` and `ES` must define every field —
//! adding one to the struct forces both to be updated.

pub struct Translations {
    // Nav / page shell
    pub files: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,

    // Drop zone
    pub drop_title: &'static str,
    pub drop_hint: &'static str,
    pub drop_browse: &'static str,
    pub uploading: &'static str,

    // File list
    pub col_name: &'static str,
    pub col_size: &'static str,
    pub col_added: &'static str,
    pub col_expires: &'static str,
    pub col_actions: &'static str,
    pub empty: &'static str,
    pub download: &'static str,
    pub delete: &'static str,
    pub delete_confirm: &'static str,
    pub storage_used: &'static str,

    // Retention settings
    pub retention_title: &'static str,
    pub retention_label: &'static str,
    pub retention_hint: &'static str,
    pub retention_save: &'static str,
    pub retention_saved: &'static str,
    pub retention_invalid: &'static str,

    // Errors
    pub err_no_file: &'static str,
    pub err_empty: &'static str,
    pub err_too_large: &'static str,
    pub err_quota: &'static str,
    pub err_disk_full: &'static str,
    pub err_failed: &'static str,
}

pub const EN: Translations = Translations {
    files: "Files",
    title: "File Clipboard",
    subtitle: "Drop files here, pick them up on any device.",

    drop_title: "Drop files here",
    drop_hint: "or",
    drop_browse: "choose files",
    uploading: "Uploading",

    col_name: "Name",
    col_size: "Size",
    col_added: "Added",
    col_expires: "Expires",
    col_actions: "",
    empty: "No files yet. Drop one above and it will show up here.",
    download: "Download",
    delete: "Delete",
    delete_confirm: "Delete this file? This cannot be undone.",
    storage_used: "used",

    retention_title: "Deletion period",
    retention_label: "Delete files after",
    retention_hint: "days. Applies to the files you already have, too.",
    retention_save: "Save",
    retention_saved: "Saved.",
    retention_invalid: "Choose between 1 and 365 days.",

    err_no_file: "No file provided.",
    err_empty: "That file is empty.",
    err_too_large: "That file is larger than the per-file limit.",
    err_quota: "That file would put you over your storage quota.",
    err_disk_full: "Not enough free space on the server.",
    err_failed: "Upload failed.",
};

pub const ES: Translations = Translations {
    files: "Archivos",
    title: "Portapapeles de archivos",
    subtitle: "Suelta archivos aquí y recógelos en cualquier dispositivo.",

    drop_title: "Suelta archivos aquí",
    drop_hint: "o",
    drop_browse: "selecciona archivos",
    uploading: "Subiendo",

    col_name: "Nombre",
    col_size: "Tamaño",
    col_added: "Añadido",
    col_expires: "Caduca",
    col_actions: "",
    empty: "Aún no hay archivos. Suelta uno arriba y aparecerá aquí.",
    download: "Descargar",
    delete: "Eliminar",
    delete_confirm: "¿Eliminar este archivo? No se puede deshacer.",
    storage_used: "usado",

    retention_title: "Periodo de eliminación",
    retention_label: "Eliminar archivos tras",
    retention_hint: "días. También se aplica a los archivos que ya tienes.",
    retention_save: "Guardar",
    retention_saved: "Guardado.",
    retention_invalid: "Elige entre 1 y 365 días.",

    err_no_file: "No se ha enviado ningún archivo.",
    err_empty: "Ese archivo está vacío.",
    err_too_large: "Ese archivo supera el límite por archivo.",
    err_quota: "Ese archivo superaría tu cuota de almacenamiento.",
    err_disk_full: "No hay espacio suficiente en el servidor.",
    err_failed: "La subida ha fallado.",
};

pub fn t(lang: myapps_core::i18n::Lang) -> &'static Translations {
    match lang {
        myapps_core::i18n::Lang::En => &EN,
        myapps_core::i18n::Lang::Es => &ES,
    }
}
