fn main() {
    // The main webview runs on a remote origin (web.whatsapp.com). Tauri only
    // lets a remote page call custom commands when the app has an ACL manifest
    // that explicitly allows them, so generate `allow-*` permissions for every
    // command registered in `generate_handler!`.
    let commands = &[
        "ping",
        "state_get",
        "settings_get",
        "settings_get_translations",
        "window_toggle",
        "escape_pressed",
        "open_settings",
        "toggle_menu",
        "toggle_privacy_blur",
        "toggle_anti_screencast",
        "open_url",
        "notify_ev",
        "notify_send",
        "log_to_main",
        "page_ready",
        "insert_text",
        "update_recent_chats",
        "update_unread",
        "get_pending_share",
        "read_file_buffer",
        "browse_folder",
        "get_default_download_paths",
        "check_path_exists",
        "settings_ignore_blur",
        "open_webstore",
        "install_extension",
        "screen_share_select",
        "settings_save",
        "branding_upload_icon",
        "branding_upload_bytes",
        "branding_reset_icons",
        "notification_reply_context",
        "notification_reply_submit",
        "notification_reply_open_chat",
        "notification_reply_cancel",
    ];

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )
    .expect("failed to run tauri-build");
}
