pub mod en;
pub mod es;

/// Supported languages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    En,
    Es,
}

impl Lang {
    pub fn from_code(code: &str) -> Self {
        match code {
            "es" => Self::Es,
            _ => Self::En,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
        }
    }
}

/// Returns the translations for the given language.
pub fn t(lang: Lang) -> &'static Translations {
    match lang {
        Lang::En => &en::EN,
        Lang::Es => &es::ES,
    }
}

/// Flat struct holding every translatable string in the application.
/// Adding a field here forces both `en.rs` and `es.rs` to be updated (compiler error).
pub struct Translations {
    // ── Shared / Layout ──────────────────────────────────────
    pub log_out: &'static str,

    // ── Login ────────────────────────────────────────────────
    pub login_title: &'static str,
    pub login_subtitle: &'static str,
    pub login_username: &'static str,
    pub login_password: &'static str,
    pub login_submit: &'static str,
    pub login_invalid: &'static str,
    pub login_error: &'static str,

    // ── Invite / Register ─────────────────────────────────────
    pub invite_title: &'static str,
    pub invite_subtitle: &'static str,
    pub invite_username: &'static str,
    pub invite_password: &'static str,
    pub invite_confirm_password: &'static str,
    pub invite_submit: &'static str,
    pub invite_invalid: &'static str,
    pub invite_expired: &'static str,
    pub invite_used: &'static str,
    pub invite_passwords_mismatch: &'static str,
    pub invite_username_taken: &'static str,

    // ── Launcher ─────────────────────────────────────────────
    pub launcher_title: &'static str,
    pub launcher_subtitle: &'static str,
    pub launcher_empty_prefix: &'static str,
    pub launcher_empty_suffix: &'static str,
    pub launcher_configure: &'static str,
    pub launcher_toggle_visibility: &'static str,
    pub launcher_done: &'static str,
    pub launcher_hide: &'static str,
    pub launcher_show: &'static str,
    pub launcher_notif_enabled: &'static str,
    pub launcher_notif_blocked: &'static str,
    pub launcher_notif_blocked_settings: &'static str,
    pub launcher_notif_enable: &'static str,

    // ── App descriptions ─────────────────────────────────────
    pub app_leanfin_desc: &'static str,
    pub app_mindflow_desc: &'static str,
    pub app_voice_desc: &'static str,
    pub app_classroom_desc: &'static str,

    // ── Language selector ────────────────────────────────────
    pub language_label: &'static str,

    // ── LeanFin nav ──────────────────────────────────────────
    pub lf_transactions: &'static str,
    pub lf_accounts: &'static str,
    pub lf_balance: &'static str,
    pub lf_expenses: &'static str,
    pub lf_labels: &'static str,
    pub lf_settings: &'static str,

    // ── LeanFin: Dashboard / Transactions page ───────────────
    pub lf_txn_title: &'static str,
    pub lf_txn_subtitle: &'static str,
    pub lf_txn_search: &'static str,
    pub lf_txn_all_accounts: &'static str,
    pub lf_txn_all_labels: &'static str,
    pub lf_txn_not_allocated: &'static str,
    pub lf_txn_loading: &'static str,
    pub lf_txn_no_transactions: &'static str,
    pub lf_txn_prev: &'static str,
    pub lf_txn_next: &'static str,
    pub lf_txn_col_date: &'static str,
    pub lf_txn_col_counterparty: &'static str,
    pub lf_txn_col_description: &'static str,
    pub lf_txn_col_labels: &'static str,
    pub lf_txn_col_amount: &'static str,
    pub lf_txn_col_balance: &'static str,
    pub lf_txn_sync: &'static str,

    // ── LeanFin: Allocations ─────────────────────────────────
    pub lf_alloc_title: &'static str,
    pub lf_alloc_remaining: &'static str,
    pub lf_alloc_choose_label: &'static str,
    pub lf_alloc_amount: &'static str,
    pub lf_alloc_add: &'static str,
    pub lf_alloc_done: &'static str,

    // ── LeanFin: Accounts page ───────────────────────────────
    pub lf_acc_title: &'static str,
    pub lf_acc_subtitle: &'static str,
    pub lf_acc_bank_accounts: &'static str,
    pub lf_acc_manual_accounts: &'static str,
    pub lf_acc_link: &'static str,
    pub lf_acc_add: &'static str,
    pub lf_acc_no_bank: &'static str,
    pub lf_acc_no_manual: &'static str,
    pub lf_acc_configure_eb: &'static str,
    pub lf_acc_show_archived: &'static str,
    pub lf_acc_archived: &'static str,
    pub lf_acc_expired: &'static str,
    pub lf_acc_active: &'static str,
    pub lf_acc_reauthorize: &'static str,
    pub lf_acc_archive: &'static str,
    pub lf_acc_unarchive: &'static str,
    pub lf_acc_delete: &'static str,
    pub lf_acc_delete_confirm_bank: &'static str,
    pub lf_acc_delete_confirm_manual: &'static str,
    pub lf_acc_update_value: &'static str,
    pub lf_acc_import_csv: &'static str,
    pub lf_acc_edit: &'static str,
    pub lf_acc_archive_error: &'static str,

    // ── LeanFin: Add Manual Account ──────────────────────────
    pub lf_acc_manual_new_title: &'static str,
    pub lf_acc_manual_new_subtitle: &'static str,
    pub lf_acc_manual_name: &'static str,
    pub lf_acc_manual_category: &'static str,
    pub lf_acc_manual_currency: &'static str,
    pub lf_acc_manual_initial: &'static str,
    pub lf_acc_manual_date: &'static str,
    pub lf_acc_manual_cancel: &'static str,
    pub lf_acc_manual_add_btn: &'static str,
    pub lf_acc_cat_investment: &'static str,
    pub lf_acc_cat_real_estate: &'static str,
    pub lf_acc_cat_vehicle: &'static str,
    pub lf_acc_cat_loan: &'static str,
    pub lf_acc_cat_crypto: &'static str,
    pub lf_acc_cat_other: &'static str,

    // ── LeanFin: Edit Account ────────────────────────────────
    pub lf_acc_edit_title: &'static str,
    pub lf_acc_edit_subtitle: &'static str,
    pub lf_acc_save_changes: &'static str,

    // ── LeanFin: Update Value ────────────────────────────────
    pub lf_acc_value_title: &'static str,
    pub lf_acc_value_new: &'static str,
    pub lf_acc_value_date: &'static str,
    pub lf_acc_value_record: &'static str,

    // ── LeanFin: Import CSV ──────────────────────────────────
    pub lf_acc_csv_title: &'static str,
    pub lf_acc_csv_file: &'static str,
    pub lf_acc_csv_format: &'static str,
    pub lf_acc_csv_format_desc: &'static str,
    pub lf_acc_csv_upload: &'static str,
    pub lf_acc_csv_import_failed: &'static str,
    pub lf_acc_csv_import_complete: &'static str,
    pub lf_acc_csv_fix_errors: &'static str,
    pub lf_acc_csv_try_again: &'static str,
    pub lf_acc_csv_back: &'static str,

    // ── LeanFin: Link Bank ───────────────────────────────────
    pub lf_acc_link_title: &'static str,
    pub lf_acc_link_subtitle: &'static str,
    pub lf_acc_link_country: &'static str,
    pub lf_acc_link_bank_name: &'static str,
    pub lf_acc_link_connect: &'static str,

    // ── LeanFin: Labels ──────────────────────────────────────
    pub lf_lbl_title: &'static str,
    pub lf_lbl_subtitle: &'static str,
    pub lf_lbl_your_labels: &'static str,
    pub lf_lbl_create: &'static str,
    pub lf_lbl_name: &'static str,
    pub lf_lbl_color: &'static str,
    pub lf_lbl_create_btn: &'static str,
    pub lf_lbl_no_labels: &'static str,
    pub lf_lbl_rules: &'static str,
    pub lf_lbl_edit: &'static str,
    pub lf_lbl_delete: &'static str,
    pub lf_lbl_delete_confirm: &'static str,
    pub lf_lbl_save: &'static str,
    pub lf_lbl_no_rules: &'static str,
    pub lf_lbl_auto_rules: &'static str,
    pub lf_lbl_counterparty: &'static str,
    pub lf_lbl_description: &'static str,
    pub lf_lbl_contains: &'static str,
    pub lf_lbl_priority: &'static str,
    pub lf_lbl_add_rule: &'static str,
    pub lf_lbl_delete_rule_confirm: &'static str,

    // ── LeanFin: Expenses ────────────────────────────────────
    pub lf_exp_title: &'static str,
    pub lf_exp_subtitle: &'static str,
    pub lf_exp_no_labels: &'static str,
    pub lf_exp_select_labels: &'static str,
    pub lf_exp_no_data: &'static str,
    pub lf_exp_no_selected: &'static str,
    pub lf_exp_transactions: &'static str,

    // ── LeanFin: Balance Evolution ───────────────────────────
    pub lf_bal_title: &'static str,
    pub lf_bal_subtitle: &'static str,
    pub lf_bal_no_accounts: &'static str,
    pub lf_bal_loading: &'static str,
    pub lf_bal_no_data: &'static str,
    pub lf_bal_account_not_found: &'static str,

    // ── LeanFin: Settings ────────────────────────────────────
    pub lf_set_title: &'static str,
    pub lf_set_subtitle: &'static str,
    pub lf_set_app_id: &'static str,
    pub lf_set_private_key: &'static str,
    pub lf_set_configured: &'static str,
    pub lf_set_not_configured: &'static str,
    pub lf_set_key_hint: &'static str,
    pub lf_set_save: &'static str,
    pub lf_set_cancel: &'static str,
    pub lf_set_encryption_warning: &'static str,
    pub lf_set_invalid_key: &'static str,
    pub lf_set_back: &'static str,

    // ── LeanFin: Sync ────────────────────────────────────────
    pub lf_sync_no_accounts: &'static str,

    // ── MindFlow nav ─────────────────────────────────────────
    pub mf_mind_map: &'static str,
    pub mf_inbox: &'static str,
    pub mf_actions: &'static str,
    pub mf_categories: &'static str,

    // ── MindFlow: Mind Map ───────────────────────────────────
    pub mf_map_title: &'static str,
    pub mf_map_subtitle: &'static str,
    pub mf_map_capture_placeholder: &'static str,
    pub mf_map_inbox_uncategorized: &'static str,
    pub mf_map_capture: &'static str,
    pub mf_map_in_inbox: &'static str,
    pub mf_map_pending: &'static str,
    pub mf_map_first_thought: &'static str,
    pub mf_map_captured: &'static str,

    // ── MindFlow: Inbox ──────────────────────────────────────
    pub mf_inbox_title: &'static str,
    pub mf_inbox_empty: &'static str,
    pub mf_inbox_move_to: &'static str,
    pub mf_inbox_move_selected: &'static str,

    // ── MindFlow: Thought detail ─────────────────────────────
    pub mf_thought_title: &'static str,
    pub mf_thought_archive: &'static str,
    pub mf_thought_unarchive: &'static str,
    pub mf_thought_archived_badge: &'static str,
    pub mf_thought_inbox_badge: &'static str,
    pub mf_thought_move: &'static str,
    pub mf_thought_comments: &'static str,
    pub mf_thought_add_comment: &'static str,
    pub mf_thought_add_btn: &'static str,
    pub mf_thought_actions: &'static str,
    pub mf_thought_new_action: &'static str,
    pub mf_thought_low: &'static str,
    pub mf_thought_medium: &'static str,
    pub mf_thought_high: &'static str,
    pub mf_thought_sub_thoughts: &'static str,
    pub mf_thought_add_sub: &'static str,

    // ── MindFlow: Categories ─────────────────────────────────
    pub mf_cat_title: &'static str,
    pub mf_cat_subtitle: &'static str,
    pub mf_cat_your_categories: &'static str,
    pub mf_cat_create: &'static str,
    pub mf_cat_name: &'static str,
    pub mf_cat_color: &'static str,
    pub mf_cat_icon: &'static str,
    pub mf_cat_icon_placeholder: &'static str,
    pub mf_cat_create_btn: &'static str,
    pub mf_cat_no_categories: &'static str,
    pub mf_cat_thoughts: &'static str,
    pub mf_cat_edit: &'static str,
    pub mf_cat_archive: &'static str,
    pub mf_cat_unarchive: &'static str,
    pub mf_cat_delete: &'static str,
    pub mf_cat_delete_confirm: &'static str,
    pub mf_cat_save: &'static str,

    // ── MindFlow: Actions ────────────────────────────────────
    pub mf_act_title: &'static str,
    pub mf_act_no_actions: &'static str,
    pub mf_act_delete_confirm: &'static str,

    // ── VoiceToText nav ──────────────────────────────────────
    pub vt_jobs: &'static str,
    pub vt_new: &'static str,

    // ── VoiceToText: Jobs page ───────────────────────────────
    pub vt_jobs_title: &'static str,
    pub vt_jobs_subtitle: &'static str,
    pub vt_jobs_new_btn: &'static str,
    pub vt_jobs_empty: &'static str,
    pub vt_jobs_col_file: &'static str,
    pub vt_jobs_col_status: &'static str,
    pub vt_jobs_col_model: &'static str,
    pub vt_jobs_col_created: &'static str,
    pub vt_jobs_col_completed: &'static str,
    pub vt_jobs_view: &'static str,
    pub vt_jobs_delete_confirm: &'static str,

    // ── VoiceToText: New transcription ───────────────────────
    pub vt_new_title: &'static str,
    pub vt_new_subtitle: &'static str,
    pub vt_new_audio_file: &'static str,
    pub vt_new_model: &'static str,
    pub vt_new_upload_btn: &'static str,
    pub vt_new_record: &'static str,
    pub vt_new_start: &'static str,
    pub vt_new_stop: &'static str,
    pub vt_new_recording: &'static str,
    pub vt_new_processing: &'static str,
    pub vt_new_no_models: &'static str,

    // ── VoiceToText: Job detail ──────────────────────────────
    pub vt_detail_not_found: &'static str,
    pub vt_detail_back: &'static str,
    pub vt_detail_file: &'static str,
    pub vt_detail_status: &'static str,
    pub vt_detail_model: &'static str,
    pub vt_detail_time: &'static str,
    pub vt_detail_created: &'static str,
    pub vt_detail_completed: &'static str,
    pub vt_detail_transcription: &'static str,
    pub vt_detail_retranscribe: &'static str,
    pub vt_detail_retranscribe_btn: &'static str,
    pub vt_detail_view_jobs: &'static str,

    // ── ClassroomInput nav ───────────────────────────────────
    pub ci_inputs: &'static str,
    pub ci_classrooms: &'static str,
    pub ci_form_types: &'static str,

    // ── ClassroomInput: Inputs page ──────────────────────────
    pub ci_inp_title: &'static str,
    pub ci_inp_subtitle: &'static str,
    pub ci_inp_new_btn: &'static str,
    pub ci_inp_empty: &'static str,
    pub ci_inp_col_name: &'static str,
    pub ci_inp_col_classroom: &'static str,
    pub ci_inp_col_form_type: &'static str,
    pub ci_inp_col_rows: &'static str,
    pub ci_inp_col_date: &'static str,
    pub ci_inp_delete_confirm: &'static str,
    pub ci_inp_delete: &'static str,
    pub ci_inp_back: &'static str,

    // ── ClassroomInput: New Input page ───────────────────────
    pub ci_inp_new_title: &'static str,
    pub ci_inp_new_subtitle: &'static str,
    pub ci_inp_classroom: &'static str,
    pub ci_inp_form_type: &'static str,
    pub ci_inp_name: &'static str,
    pub ci_inp_save: &'static str,
    pub ci_inp_need_both: &'static str,
    pub ci_inp_need_classroom: &'static str,
    pub ci_inp_need_form_type: &'static str,
    pub ci_inp_select_hint: &'static str,
    pub ci_inp_not_found: &'static str,
    pub ci_inp_pupil: &'static str,

    // ── ClassroomInput: Classrooms page ──────────────────────
    pub ci_cls_title: &'static str,
    pub ci_cls_subtitle: &'static str,
    pub ci_cls_your_classrooms: &'static str,
    pub ci_cls_add: &'static str,
    pub ci_cls_label: &'static str,
    pub ci_cls_pupils: &'static str,
    pub ci_cls_pupils_hint: &'static str,
    pub ci_cls_create_btn: &'static str,
    pub ci_cls_no_classrooms: &'static str,
    pub ci_cls_delete_confirm: &'static str,
    pub ci_cls_pupils_count: &'static str,

    // ── ClassroomInput: Form Types page ──────────────────────
    pub ci_ft_title: &'static str,
    pub ci_ft_subtitle: &'static str,
    pub ci_ft_your_types: &'static str,
    pub ci_ft_create: &'static str,
    pub ci_ft_name: &'static str,
    pub ci_ft_columns: &'static str,
    pub ci_ft_col_name: &'static str,
    pub ci_ft_col_text: &'static str,
    pub ci_ft_col_number: &'static str,
    pub ci_ft_col_bool: &'static str,
    pub ci_ft_add_column: &'static str,
    pub ci_ft_create_btn: &'static str,
    pub ci_ft_no_types: &'static str,
    pub ci_ft_delete_confirm: &'static str,
    pub ci_ft_edit: &'static str,
    pub ci_ft_edit_title: &'static str,
    pub ci_ft_save: &'static str,
    pub ci_ft_cancel: &'static str,
    pub ci_ft_no_columns: &'static str,
    pub ci_ft_not_found: &'static str,

    // ── Command Bar ─────────────────────────────────────────
    pub cmd_placeholder: &'static str,
    pub cmd_busy: &'static str,
    pub cmd_error: &'static str,
    pub cmd_not_configured: &'static str,
    pub cmd_no_actions: &'static str,
    pub cmd_no_params: &'static str,
    pub cmd_confidence: &'static str,
    pub cmd_confirm: &'static str,
    pub cmd_cancel: &'static str,

    // ── Command Bar (voice) ──────────────────────────────────
    pub cmd_record: &'static str,
    pub cmd_voice_command: &'static str,
    pub cmd_recording: &'static str,
    pub cmd_transcribing: &'static str,
    pub cmd_interpreting: &'static str,
    pub cmd_edit: &'static str,
    pub cmd_edit_done: &'static str,
    pub cmd_transcribe_error: &'static str,
    pub cmd_mic_no_permission: &'static str,
}

impl Translations {
    /// Return the translated app description for a given registry key.
    pub fn app_description(&self, key: &str) -> &'static str {
        match key {
            "leanfin" => self.app_leanfin_desc,
            "mindflow" => self.app_mindflow_desc,
            "voice_to_text" => self.app_voice_desc,
            "classroom_input" => self.app_classroom_desc,
            _ => "",
        }
    }
}
