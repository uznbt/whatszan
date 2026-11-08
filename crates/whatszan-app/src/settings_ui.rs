//! Native GTK settings window.
//!
//! The Electron build rendered `static/settings.html` inside a BrowserWindow.
//! This module replaces that page with GTK widgets built entirely in Rust. It
//! shares `commands::save_settings` with the IPC path, so both frontends apply
//! identical validation and side effects.

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use gtk::prelude::*;
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager};
use whatszan_core::translations::t;

use crate::{app_state::AppState, commands};

thread_local! {
    static SETTINGS_WINDOW: RefCell<Option<gtk::Window>> = RefCell::new(None);
    static CSS_INSTALLED: Cell<bool> = Cell::new(false);
}

/// Open the settings window, or present the existing one.
pub fn open(app: &AppHandle) {
    let app = app.clone();
    let scheduled = app.clone();
    if let Err(err) = app.run_on_main_thread(move || present_or_create(&scheduled)) {
        tracing::error!(%err, "cannot schedule settings window");
    }
}

fn present_or_create(app: &AppHandle) {
    let existing = SETTINGS_WINDOW.with(|slot| slot.borrow().clone());
    if let Some(window) = existing {
        if window.is_visible() {
            window.present();
            return;
        }
    }

    let Some(window) = build_window(app) else {
        return;
    };

    SETTINGS_WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
    window.connect_destroy(|_| {
        SETTINGS_WINDOW.with(|slot| *slot.borrow_mut() = None);
    });
    window.show_all();
    window.present();
}

fn build_window(app: &AppHandle) -> Option<gtk::Window> {
    install_css();

    let state = app.state::<AppState>();
    let settings = state.settings_value();
    let translations = state.translations();

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title(&tr(app, "settings_header_title", "Pengaturan"));
    window.set_default_size(880, 660);
    window.set_position(gtk::WindowPosition::Center);
    window.set_resizable(true);
    window.set_skip_taskbar_hint(true);
    // A dialog hint keeps the window floating (and at its requested 880×660)
    // on tiling compositors such as Hyprland instead of squeezing the settings
    // cards into a tiled column.
    window.set_type_hint(gtk::gdk::WindowTypeHint::Dialog);
    window.style_context().add_class("wz-window");
    window.style_context().add_class(if state.is_theme_dark() {
        "wz-dark"
    } else {
        "wz-light"
    });
    let app_icon =
        gtk::gdk_pixbuf::Pixbuf::from_read(std::io::Cursor::new(crate::assets::APP_ICON_PNG)).ok();
    if let Some(icon) = app_icon.as_ref() {
        window.set_icon(Some(icon));
    }

    if let Some(main) = app
        .get_webview_window("main")
        .and_then(|w| w.gtk_window().ok())
    {
        window.set_transient_for(Some(&main));
    }

    let header = header(app, app_icon.as_ref());
    let tabs = SettingsTabs::new();

    // ---------------------------------------------------------------- general
    let general = page();
    section(&general, &tr(app, "settings_tab_general", "Umum"));

    let app_language = gtk::ComboBoxText::new();
    app_language.append(
        Some("auto"),
        &tr(app, "settings_placeholder_auto", "Otomatis"),
    );
    app_language.append(Some("en"), "English");
    app_language.append(Some("id"), "Bahasa Indonesia");
    app_language.append(Some("de"), "Deutsch");
    app_language.append(Some("fr"), "Français");
    app_language.append(Some("it"), "Italiano");
    app_language.append(Some("pt"), "Português");
    general.pack_start(
        &row(
            &tr(app, "settings_language_title", "Bahasa Aplikasi"),
            Some(&tr(
                app,
                "settings_language_desc",
                "Pilih bahasa untuk pengaturan dan menu konteks",
            )),
            &app_language,
        ),
        false,
        false,
        0,
    );

    let auto_run = gtk::Switch::new();
    general.pack_start(
        &row(
            &tr(
                app,
                "settings_autorun_title",
                "Jalankan Otomatis saat Startup",
            ),
            Some(&tr(
                app,
                "settings_autorun_desc",
                "Jalankan otomatis saat startup",
            )),
            &auto_run,
        ),
        false,
        false,
        0,
    );

    let esc_toggle = gtk::Switch::new();
    general.pack_start(
        &row(
            &tr(app, "settings_esc_title", "Sembunyikan dengan ESC"),
            Some(&tr(app, "settings_esc_desc", "Tekan ESC untuk sembunyikan")),
            &esc_toggle,
        ),
        false,
        false,
        0,
    );

    let close_to_tray = gtk::Switch::new();
    general.pack_start(
        &row(
            &tr(app, "settings_quit_tray_title", "Tutup ke Tray"),
            Some(&tr(
                app,
                "settings_quit_tray_desc",
                "Aplikasi ke tray saat ditutup",
            )),
            &close_to_tray,
        ),
        false,
        false,
        0,
    );

    let smart_download = gtk::Switch::new();
    general.pack_start(
        &row(
            &tr(app, "settings_smart_dl_title", "Pilah Unduhan Otomatis"),
            Some(&tr(
                app,
                "settings_smart_dl_desc",
                "Simpan file ke folder yang sesuai otomatis",
            )),
            &smart_download,
        ),
        false,
        false,
        0,
    );

    let smart_paths = gtk::Box::new(gtk::Orientation::Vertical, 8);
    smart_paths.set_no_show_all(true);
    smart_paths.set_margin_start(20);
    smart_paths.set_margin_top(4);
    let (image_box, download_path_image, browse_image) = path_field(
        app,
        &window,
        &tr(app, "settings_smart_dl_path_image_title", "Foto & Gambar"),
        &tr(
            app,
            "settings_smart_dl_path_image_desc",
            "Folder Gambar sistem",
        ),
    );
    let (video_box, download_path_video, browse_video) = path_field(
        app,
        &window,
        &tr(app, "settings_smart_dl_path_video_title", "Video"),
        &tr(
            app,
            "settings_smart_dl_path_video_desc",
            "Folder Video sistem",
        ),
    );
    let (audio_box, download_path_audio, browse_audio) = path_field(
        app,
        &window,
        &tr(app, "settings_smart_dl_path_audio_title", "Audio"),
        &tr(
            app,
            "settings_smart_dl_path_audio_desc",
            "Folder Musik sistem",
        ),
    );
    let (document_box, download_path_document, browse_document) = path_field(
        app,
        &window,
        &tr(app, "settings_smart_dl_path_document_title", "Dokumen"),
        &tr(
            app,
            "settings_smart_dl_path_document_desc",
            "Folder Dokumen sistem",
        ),
    );
    smart_paths.pack_start(&image_box, false, false, 0);
    smart_paths.pack_start(&video_box, false, false, 0);
    smart_paths.pack_start(&audio_box, false, false, 0);
    smart_paths.pack_start(&document_box, false, false, 0);
    general.pack_start(&smart_paths, false, false, 0);

    let notif_prefix = gtk::Entry::new();
    notif_prefix.set_placeholder_text(Some(&tr(
        app,
        "settings_placeholder_notif",
        "Kosongkan jika tidak perlu",
    )));
    field(
        &general,
        &tr(app, "settings_notif_prefix_title", "Awalan Notifikasi"),
        Some(&tr(
            app,
            "settings_notif_prefix_desc",
            "Awalan teks pada notifikasi",
        )),
        &notif_prefix,
    );
    add_page(
        &tabs,
        &general,
        &tr(app, "settings_tab_general", "Umum"),
        "emblem-system-symbolic",
    );

    // ---------------------------------------------------------------- privacy
    let privacy = page();
    section(&privacy, &tr(app, "settings_tab_privacy", "Privasi"));

    let privacy_blur = gtk::Switch::new();
    privacy.pack_start(
        &row(
            &tr(app, "settings_privacy_blur_title", "Aktifkan Mode Rahasia"),
            Some(&tr(
                app,
                "settings_privacy_blur_desc",
                "Aktifkan sensor layar. Tekan Ctrl+Shift+B kapan saja.",
            )),
            &privacy_blur,
        ),
        false,
        false,
        0,
    );

    let blur_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    blur_box.set_no_show_all(true);
    blur_box.set_margin_start(20);
    let blur_contacts = gtk::Switch::new();
    let blur_pp = gtk::Switch::new();
    let blur_messages = gtk::Switch::new();
    let blur_media = gtk::Switch::new();
    let blur_input = gtk::Switch::new();
    for (title, switch) in [
        (
            &tr(app, "settings_blur_contacts", "Blur Daftar Kontak"),
            &blur_contacts,
        ),
        (&tr(app, "settings_blur_pp", "Blur Foto Profil"), &blur_pp),
        (
            &tr(app, "settings_blur_messages", "Blur Isi Pesan"),
            &blur_messages,
        ),
        (&tr(app, "settings_blur_media", "Blur Media"), &blur_media),
        (
            &tr(app, "settings_blur_input", "Blur Teks Ketikan"),
            &blur_input,
        ),
    ] {
        blur_box.pack_start(&row(title, None, switch), false, false, 0);
    }

    let blur_amount = gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, 15.0, 1.0);
    blur_amount.set_draw_value(true);
    blur_amount.set_digits(0);
    blur_amount.set_hexpand(true);
    field(
        &blur_box,
        &tr(app, "settings_blur_amount_title", "Tingkat Blur"),
        Some(&tr(
            app,
            "settings_blur_amount_desc",
            "Geser untuk mengatur seberapa buram efek sensornya",
        )),
        &blur_amount,
    );

    let unblur_delay = gtk::SpinButton::with_range(0.0, 60.0, 0.1);
    unblur_delay.set_digits(1);
    unblur_delay.set_numeric(true);
    field(
        &blur_box,
        &tr(app, "settings_unblur_delay_title", "Tunda Unblur (Detik)"),
        Some(&tr(
            app,
            "settings_unblur_delay_desc",
            "Jeda sebelum sensor terbuka saat kursor diarahkan ke sana",
        )),
        &unblur_delay,
    );

    let blur_specific = gtk::Entry::new();
    blur_specific.set_placeholder_text(Some(&tr(
        app,
        "settings_placeholder_specific",
        "Contoh: Bos, Budi",
    )));
    field(
        &blur_box,
        &tr(app, "settings_blur_specific_title", "Hanya Kontak Spesifik"),
        Some(&tr(
            app,
            "settings_blur_specific_desc",
            "Sensor hanya berlaku untuk nama-nama ini. Pisahkan dengan koma.",
        )),
        &blur_specific,
    );
    privacy.pack_start(&blur_box, false, false, 0);

    let anti_screencast = gtk::Switch::new();
    privacy.pack_start(
        &row(
            &tr(
                app,
                "settings_anti_screencast_title",
                "Anti Rekam Layar / Screenshot",
            ),
            Some(&tr(
                app,
                "settings_anti_screencast_desc",
                "Sembunyikan aplikasi dari rekam layar / monitor eksternal. Tekan Ctrl+Shift+H kapan saja.",
            )),
            &anti_screencast,
        ),
        false,
        false,
        0,
    );

    section(
        &privacy,
        &tr(app, "settings_incognito_title", "Incognito / Power User"),
    );
    let incognito_noread = gtk::Switch::new();
    let incognito_notyping = gtk::Switch::new();
    let incognito_antidelete = gtk::Switch::new();
    let incognito_statussaver = gtk::Switch::new();
    for (title, desc, switch) in [
        (
            tr(app, "settings_ghost_read_title", "Ghost Mode: Centang Biru"),
            tr(
                app,
                "settings_ghost_read_desc",
                "Lawan bicara tidak tahu pesannya sudah kamu baca",
            ),
            &incognito_noread,
        ),
        (
            tr(
                app,
                "settings_ghost_typing_title",
                "Ghost Mode: Sembunyikan Mengetik",
            ),
            tr(
                app,
                "settings_ghost_typing_desc",
                "Sembunyikan indikator sedang mengetik di HP lawan bicara",
            ),
            &incognito_notyping,
        ),
        (
            tr(app, "settings_antidelete_title", "Anti-Hapus Pesan"),
            tr(
                app,
                "settings_antidelete_desc",
                "Pulihkan pesan yang ditarik oleh pengirim",
            ),
            &incognito_antidelete,
        ),
        (
            tr(
                app,
                "settings_statussaver_title",
                "Pengunduh Status (Status Saver)",
            ),
            tr(
                app,
                "settings_statussaver_desc",
                "Tampilkan tombol simpan saat melihat Story teman",
            ),
            &incognito_statussaver,
        ),
    ] {
        privacy.pack_start(&row(&title, Some(&desc), switch), false, false, 0);
    }
    add_page(
        &tabs,
        &privacy,
        &tr(app, "settings_tab_privacy", "Privasi"),
        "security-high-symbolic",
    );

    // ------------------------------------------------------------- dictionary
    let dictionary = page();
    section(
        &dictionary,
        &tr(app, "settings_dict_title", "Kamus Otomatis"),
    );
    let dict_desc = gtk::Label::new(None);
    dict_desc.set_text(&tr(
        app,
        "settings_dict_desc",
        "Ganti kata pintasan menjadi teks panjang secara otomatis saat mengetik di obrolan.",
    ));
    dict_desc.set_xalign(0.0);
    dict_desc.set_line_wrap(true);
    dict_desc.style_context().add_class("wz-desc");
    dictionary.pack_start(&dict_desc, false, false, 4);

    let dict_trigger = gtk::Entry::new();
    dict_trigger.set_placeholder_text(Some(&tr(
        app,
        "settings_placeholder_trigger",
        "Pintasan (misal: npm)",
    )));
    dict_trigger.set_hexpand(true);
    let dict_replacement = gtk::Entry::new();
    dict_replacement.set_placeholder_text(Some(&tr(
        app,
        "settings_placeholder_replacement",
        "Teks pengganti (misal: 123...)",
    )));
    dict_replacement.set_hexpand(true);
    let dict_add = gtk::Button::with_label(&tr(app, "settings_btn_add", "Tambah"));
    dict_add.style_context().add_class("wz-primary");
    let dict_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    dict_row.style_context().add_class("wz-card");
    dict_row.pack_start(&dict_trigger, true, true, 0);
    dict_row.pack_start(&dict_replacement, true, true, 0);
    dict_row.pack_end(&dict_add, false, false, 0);
    dictionary.pack_start(&dict_row, false, false, 2);

    let dict_list = gtk::ListBox::new();
    dict_list.set_selection_mode(gtk::SelectionMode::None);
    dict_list.set_margin_top(4);
    dict_list.style_context().add_class("wz-dict-list");
    dictionary.pack_start(&dict_list, false, false, 0);
    add_page(
        &tabs,
        &dictionary,
        &tr(app, "settings_tab_dictionary", "Kamus"),
        "accessories-dictionary-symbolic",
    );

    // -------------------------------------------------------------- appearance
    let appearance = page();
    section(&appearance, &tr(app, "settings_tab_appearance", "Tampilan"));
    let menu_bar = gtk::Switch::new();
    let menu_bar_auto_hide = gtk::Switch::new();
    let spellcheck = gtk::Switch::new();
    let low_resource_mode = gtk::Switch::new();
    let disable_gpu = gtk::Switch::new();
    for (title, desc, switch) in [
        (
            tr(app, "settings_menubar_title", "Tampilkan Menu Bar"),
            tr(
                app,
                "settings_menubar_desc",
                "Menampilkan menu aplikasi di bagian atas jendela",
            ),
            &menu_bar,
        ),
        (
            tr(
                app,
                "settings_menubar_autohide_title",
                "Sembunyikan Menu Bar Otomatis",
            ),
            tr(
                app,
                "settings_menubar_autohide_desc",
                "Menu bar hanya muncul saat tombol Alt ditekan",
            ),
            &menu_bar_auto_hide,
        ),
        (
            tr(app, "settings_spellcheck_title", "Pemeriksa Ejaan"),
            tr(
                app,
                "settings_spellcheck_desc",
                "Garis bawah merah pada kata yang salah ketik",
            ),
            &spellcheck,
        ),
        (
            tr(app, "settings_lowresource_title", "Mode Sumber Daya Rendah"),
            tr(
                app,
                "settings_lowresource_desc",
                "Matikan layanan latar belakang Chromium yang tidak dipakai WhatsApp.",
            ),
            &low_resource_mode,
        ),
        (
            tr(app, "settings_disablegpu_title", "Matikan Akselerasi GPU"),
            tr(
                app,
                "settings_disablegpu_desc",
                "Rendering dipindah ke CPU. Perubahan berlaku setelah restart.",
            ),
            &disable_gpu,
        ),
    ] {
        appearance.pack_start(&row(&title, Some(&desc), switch), false, false, 0);
    }
    add_page(
        &tabs,
        &appearance,
        &tr(app, "settings_tab_appearance", "Tampilan"),
        "preferences-desktop-display-symbolic",
    );

    // ---------------------------------------------------------------- advanced
    let advanced = page();
    section(&advanced, &tr(app, "settings_tab_advanced", "Lanjutan"));
    let webstore = gtk::Button::with_label(&tr(app, "settings_btn_webstore", "Buka Web Store"));
    advanced.pack_start(
        &row(
            &tr(app, "settings_chrome_ext_title", "Ekstensi Chrome"),
            Some(&tr(
                app,
                "settings_chrome_ext_desc",
                "Ekstensi browser tidak didukung oleh WebKitGTK.",
            )),
            &webstore,
        ),
        false,
        false,
        0,
    );
    let user_agent = gtk::Entry::new();
    user_agent.set_placeholder_text(Some(&tr(app, "settings_placeholder_auto", "Otomatis")));
    field(
        &advanced,
        &tr(app, "settings_ua_title", "User Agent Kustom"),
        Some(&tr(
            app,
            "settings_ua_desc",
            "String identifikasi perangkat. Kosongkan untuk default.",
        )),
        &user_agent,
    );
    add_page(
        &tabs,
        &advanced,
        &tr(app, "settings_tab_advanced", "Lanjutan"),
        "preferences-other-symbolic",
    );

    // ---------------------------------------------------------------- branding
    let branding = page();
    section(&branding, "Branding & Shortcut");

    let custom_app_name = gtk::Entry::new();
    custom_app_name.set_placeholder_text(Some("WhatsZan"));
    branding.pack_start(
        &row(
            &tr(app, "settings_branding_name_title", "Nama Aplikasi"),
            Some(&tr(
                app,
                "settings_branding_name_desc",
                "Nama yang muncul di dalam obrolan dan judul jendela",
            )),
            &custom_app_name,
        ),
        false,
        false,
        0,
    );

    let desktop_shortcut = gtk::Switch::new();
    branding.pack_start(
        &row(
            &tr(app, "settings_branding_desktop_title", "Pintasan Desktop"),
            Some(&tr(
                app,
                "settings_branding_desktop_desc",
                "Tampilkan ikon aplikasi di Desktop",
            )),
            &desktop_shortcut,
        ),
        false,
        false,
        0,
    );

    let icon_app = IconPicker::new(app, "app");
    let icon_unread = IconPicker::new(app, "unread");
    let icon_normal = IconPicker::new(app, "normal");
    let (icon_app_box, icon_app_choose) = icon_field(
        app,
        &tr(app, "settings_branding_app_title", "Ikon Aplikasi"),
        &tr(
            app,
            "settings_branding_app_desc",
            "Ikon untuk shortcut dan pencarian.",
        ),
        &icon_app,
    );
    let (icon_unread_box, icon_unread_choose) = icon_field(
        app,
        &tr(
            app,
            "settings_branding_tray_unread_title",
            "Ikon Tray (Ada Pesan)",
        ),
        &tr(
            app,
            "settings_branding_tray_unread_desc",
            "Ikon tray saat ada pesan masuk.",
        ),
        &icon_unread,
    );
    let (icon_normal_box, icon_normal_choose) = icon_field(
        app,
        &tr(
            app,
            "settings_branding_tray_normal_title",
            "Ikon Tray (Normal)",
        ),
        &tr(
            app,
            "settings_branding_tray_normal_desc",
            "Ikon tray saat tidak ada pesan.",
        ),
        &icon_normal,
    );
    branding.pack_start(&icon_app_box, false, false, 0);
    branding.pack_start(&icon_unread_box, false, false, 0);
    branding.pack_start(&icon_normal_box, false, false, 0);

    let reset_branding =
        gtk::Button::with_label(&tr(app, "settings_branding_reset", "Reset Semua Ikon"));
    let reset_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    reset_row.set_margin_top(8);
    reset_row.pack_end(&reset_branding, false, false, 0);
    branding.pack_start(&reset_row, false, false, 0);
    add_page(
        &tabs,
        &branding,
        &tr(app, "settings_tab_branding", "Branding"),
        "image-x-generic-symbolic",
    );

    let status = gtk::Label::new(None);
    status.set_xalign(0.5);
    status.set_halign(gtk::Align::Center);
    status.set_margin_top(8);
    status.set_margin_bottom(10);
    status.style_context().add_class("wz-status");
    status.set_no_show_all(true);
    status.hide();

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    content.style_context().add_class("wz-content");
    content.pack_start(&tabs.sidebar, false, false, 0);
    content.pack_start(&tabs.stack, true, true, 0);

    let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    outer.pack_start(&header, false, false, 0);
    outer.pack_start(&content, true, true, 0);
    outer.pack_end(&status, false, false, 0);
    window.add(&outer);

    let form = Rc::new(Form {
        app: app.clone(),
        window: window.clone(),
        status,
        app_language,
        auto_run,
        esc_toggle,
        close_to_tray,
        smart_download,
        smart_paths,
        download_path_image,
        download_path_video,
        download_path_audio,
        download_path_document,
        notif_prefix,
        privacy_blur,
        blur_box,
        blur_contacts,
        blur_pp,
        blur_messages,
        blur_media,
        blur_input,
        blur_amount,
        unblur_delay,
        blur_specific,
        anti_screencast,
        incognito_noread,
        incognito_notyping,
        incognito_antidelete,
        incognito_statussaver,
        dict_trigger,
        dict_replacement,
        dict_add,
        dict_list,
        text_replacements: RefCell::new(Map::new()),
        menu_bar,
        menu_bar_auto_hide,
        spellcheck,
        low_resource_mode,
        disable_gpu,
        user_agent,
        webstore,
        custom_app_name,
        desktop_shortcut,
        icon_app,
        icon_unread,
        icon_normal,
        icon_last_app: Rc::new(RefCell::new("whatszan".into())),
        icon_last_unread: Rc::new(RefCell::new("whatszan".into())),
        icon_last_normal: Rc::new(RefCell::new("whatszan".into())),
        icon_app_choose,
        icon_unread_choose,
        icon_normal_choose,
        reset_branding,
        updating: Cell::new(false),
        sidebar_width: Cell::new(220),
        status_token: Cell::new(0),
        last_language: RefCell::new(String::new()),
    });

    load_values(&form, &settings, &translations);
    connect_browse(&form, &browse_image, &form.download_path_image);
    connect_browse(&form, &browse_video, &form.download_path_video);
    connect_browse(&form, &browse_audio, &form.download_path_audio);
    connect_browse(&form, &browse_document, &form.download_path_document);

    let saver = Saver::new(form.clone());
    connect_signals(&form, &saver);
    render_dict_list(&form, &saver);

    // Closing when the window loses focus mirrors the Electron popup. A short
    // grace period avoids closing while the window is still being mapped, and
    // native file choosers set `settings_ignore_blur` while they are open.
    //
    // `--settings` is a standalone launch with a hidden main window. There is
    // no parent to return focus to, and some compositors never give the
    // transient dialog focus in that state, so keep it open until the user
    // closes it in that mode.
    if !std::env::args().any(|arg| arg == "--settings") {
        let shown_at = Rc::new(Cell::new(Instant::now()));
        let shown_at_focus = shown_at.clone();
        let focus_app = app.clone();
        window.connect_focus_out_event(move |window, _| {
            if shown_at_focus.get().elapsed() < Duration::from_millis(700) {
                return gtk::glib::Propagation::Proceed;
            }
            let state = focus_app.state::<AppState>();
            if !state.settings_ignore_blur() {
                window.close();
            }
            gtk::glib::Propagation::Proceed
        });
        shown_at.set(Instant::now());
    }

    Some(window)
}

fn header(app: &AppHandle, icon: Option<&gtk::gdk_pixbuf::Pixbuf>) -> gtk::Box {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    bar.style_context().add_class("wz-header");

    let icon_holder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    icon_holder.set_size_request(36, 36);
    icon_holder.set_valign(gtk::Align::Center);
    icon_holder.style_context().add_class("wz-header-icon");

    // `gtk_image_set_pixel_size` only affects named icons; a pixbuf keeps its
    // natural size. Scale the 512px embedded icon so it cannot take over the
    // whole header.
    let scaled =
        icon.and_then(|pixbuf| pixbuf.scale_simple(22, 22, gtk::gdk_pixbuf::InterpType::Bilinear));
    let logo = scaled
        .as_ref()
        .map(|pixbuf| gtk::Image::from_pixbuf(Some(pixbuf)))
        .unwrap_or_else(|| {
            gtk::Image::from_icon_name(Some("preferences-system"), gtk::IconSize::Dnd)
        });
    logo.set_halign(gtk::Align::Center);
    logo.set_valign(gtk::Align::Center);
    icon_holder.pack_start(&logo, true, true, 0);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
    text.set_valign(gtk::Align::Center);

    let title = gtk::Label::new(None);
    title.set_text(&tr(app, "settings_header_title", "Pengaturan"));
    title.set_xalign(0.0);
    title.style_context().add_class("wz-header-title");

    let subtitle = gtk::Label::new(None);
    subtitle.set_text(&tr(
        app,
        "settings_header_subtitle",
        "WhatsZan Desktop Client",
    ));
    subtitle.set_xalign(0.0);
    subtitle.style_context().add_class("wz-header-subtitle");

    text.pack_start(&title, false, false, 0);
    text.pack_start(&subtitle, false, false, 0);

    bar.pack_start(&icon_holder, false, false, 0);
    bar.pack_start(&text, false, false, 0);
    bar
}

/// Sidebar plus page stack. GTK's notebook can be themed, but a stack gives
/// the settings window the same left-hand tab list as the Electron page
/// without the notebook frame around the content.
struct SettingsTabs {
    sidebar: gtk::Box,
    stack: gtk::Stack,
    buttons: Rc<RefCell<Vec<gtk::ToggleButton>>>,
    /// `gtk_toggle_button_set_active()` emits `toggled` (and, in GTK 3,
    /// `clicked`) again. Syncing the other buttons from inside a handler would
    /// otherwise recurse and bounce the stack between pages forever.
    syncing: Rc<Cell<bool>>,
}

impl SettingsTabs {
    fn new() -> Self {
        let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 2);
        sidebar.set_size_request(200, -1);
        sidebar.style_context().add_class("wz-sidebar");

        let stack = gtk::Stack::new();
        stack.set_hexpand(true);
        stack.set_vexpand(true);
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.set_transition_duration(120);
        stack.style_context().add_class("wz-stack");

        Self {
            sidebar,
            stack,
            buttons: Rc::new(RefCell::new(Vec::new())),
            syncing: Rc::new(Cell::new(false)),
        }
    }

    fn add_page(&self, content: &gtk::Box, title: &str, icon_name: &str) {
        let scrolled = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scrolled.set_hexpand(true);
        scrolled.set_vexpand(true);
        scrolled.style_context().add_class("wz-scroll");
        scrolled.add(content);

        let name = format!("page-{}", self.buttons.borrow().len());
        self.stack.add_named(&scrolled, &name);

        let button = gtk::ToggleButton::new();
        button.set_relief(gtk::ReliefStyle::None);
        button.set_hexpand(true);
        button.style_context().add_class("wz-tab");

        let line = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        line.set_hexpand(true);
        let has_icon = gtk::IconTheme::default()
            .map(|theme| theme.has_icon(icon_name))
            .unwrap_or(false);
        if has_icon {
            let icon = gtk::Image::from_icon_name(Some(icon_name), gtk::IconSize::Menu);
            icon.set_pixel_size(18);
            icon.style_context().add_class("wz-tab-icon");
            line.pack_start(&icon, false, false, 0);
        }
        let label = gtk::Label::new(Some(title));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        line.pack_start(&label, true, true, 0);
        button.add(&line);

        {
            let buttons = self.buttons.clone();
            let stack = self.stack.clone();
            let syncing = self.syncing.clone();
            let page_name = name.clone();
            button.connect_toggled(move |clicked| {
                if syncing.get() {
                    return;
                }

                if !clicked.is_active() {
                    // Clicking the already selected tab deactivates it. Keep it
                    // selected instead of leaving the sidebar without a page.
                    if stack.visible_child_name().as_deref() == Some(page_name.as_str()) {
                        syncing.set(true);
                        clicked.set_active(true);
                        syncing.set(false);
                    }
                    return;
                }

                syncing.set(true);
                stack.set_visible_child_name(&page_name);
                for other in buttons.borrow().iter() {
                    if other != clicked {
                        other.set_active(false);
                    }
                }
                syncing.set(false);
            });
        }

        let first = self.buttons.borrow().is_empty();
        self.buttons.borrow_mut().push(button.clone());
        self.sidebar.pack_start(&button, false, false, 0);

        if first {
            button.set_active(true);
            self.stack.set_visible_child_name(&name);
        }
    }
}

fn page() -> gtk::Box {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_margin_top(20);
    content.set_margin_bottom(24);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content
}

fn add_page(tabs: &SettingsTabs, content: &gtk::Box, title: &str, icon_name: &str) {
    tabs.add_page(content, title, icon_name);
}

fn section(container: &gtk::Box, title: &str) {
    let label = gtk::Label::new(None);
    label.set_text(title);
    label.set_xalign(0.0);
    label.set_margin_top(6);
    label.style_context().add_class("wz-section");
    container.pack_start(&label, false, false, 2);
}

fn row(title: &str, desc: Option<&str>, control: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.style_context().add_class("wz-card");

    let info = gtk::Box::new(gtk::Orientation::Vertical, 2);
    info.set_hexpand(true);
    info.set_valign(gtk::Align::Center);

    let title_label = gtk::Label::new(None);
    title_label.set_text(title);
    title_label.set_xalign(0.0);
    title_label.set_line_wrap(true);
    title_label.style_context().add_class("wz-title");
    info.pack_start(&title_label, false, false, 0);

    if let Some(desc) = desc {
        let desc_label = gtk::Label::new(None);
        desc_label.set_text(desc);
        desc_label.set_xalign(0.0);
        desc_label.set_line_wrap(true);
        desc_label.style_context().add_class("wz-desc");
        info.pack_start(&desc_label, false, false, 0);
    }

    control.set_valign(gtk::Align::Center);
    row.pack_start(&info, true, true, 0);
    row.pack_end(control, false, false, 0);
    row
}

fn field(container: &gtk::Box, title: &str, desc: Option<&str>, control: &impl IsA<gtk::Widget>) {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 3);
    card.style_context().add_class("wz-card");

    let title_label = gtk::Label::new(None);
    title_label.set_text(title);
    title_label.set_xalign(0.0);
    title_label.style_context().add_class("wz-title");
    card.pack_start(&title_label, false, false, 0);

    if let Some(desc) = desc {
        let desc_label = gtk::Label::new(None);
        desc_label.set_text(desc);
        desc_label.set_xalign(0.0);
        desc_label.set_line_wrap(true);
        desc_label.style_context().add_class("wz-desc");
        card.pack_start(&desc_label, false, false, 0);
    }
    card.pack_start(control, false, false, 2);
    container.pack_start(&card, false, false, 0);
}

fn path_field(
    app: &AppHandle,
    parent: &gtk::Window,
    title: &str,
    desc: &str,
) -> (gtk::Box, gtk::Entry, gtk::Button) {
    let entry = gtk::Entry::new();
    entry.set_hexpand(true);
    entry.set_placeholder_text(Some(desc));

    let browse = gtk::Button::with_label(&tr(app, "settings_btn_browse", "Browse"));
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    line.pack_start(&entry, true, true, 0);
    line.pack_end(&browse, false, false, 0);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
    container.style_context().add_class("wz-card");
    let label = gtk::Label::new(None);
    label.set_text(title);
    label.set_xalign(0.0);
    label.style_context().add_class("wz-title");
    container.pack_start(&label, false, false, 0);
    container.pack_start(&line, false, false, 0);

    let _ = parent;
    (container, entry, browse)
}

#[derive(Clone)]
struct IconPicker {
    kind: &'static str,
    container: gtk::Box,
    buttons: Vec<(String, gtk::ToggleButton)>,
    custom_image: gtk::Image,
}

impl IconPicker {
    fn new(app: &AppHandle, kind: &'static str) -> Self {
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        container.style_context().add_class("wz-icon-picker");

        let default_icon = if kind == "unread" {
            crate::assets::APP_BADGED_ICON_PNG
        } else {
            crate::assets::APP_ICON_PNG
        };
        let app_image = icon_image(default_icon, 32);
        let whatsapp_image = icon_image(crate::assets::WHATSAPP_ICON_PNG, 32);
        let custom_image = gtk::Image::new();
        set_custom_thumbnail(&custom_image, None);

        let choices = [
            ("whatszan", "WhatsZan".to_string(), app_image),
            ("whatsapp", "WhatsApp".to_string(), whatsapp_image),
            (
                "custom",
                tr(app, "settings_branding_custom", "Kustom"),
                custom_image.clone(),
            ),
        ];

        let mut buttons: Vec<(String, gtk::ToggleButton)> = Vec::new();
        for (id, label, image) in choices {
            let button = gtk::ToggleButton::new();
            button.set_relief(gtk::ReliefStyle::None);
            button.style_context().add_class("wz-icon-option");

            let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
            content.set_halign(gtk::Align::Center);
            content.set_valign(gtk::Align::Center);
            content.pack_start(&image, false, false, 0);
            let caption = gtk::Label::new(Some(&label));
            caption.style_context().add_class("wz-icon-caption");
            content.pack_start(&caption, false, false, 0);
            button.add(&content);
            button.show_all();
            container.pack_start(&button, false, false, 0);
            buttons.push((id.to_string(), button));
        }
        container.show_all();

        let picker = Self {
            kind,
            container,
            buttons,
            custom_image,
        };
        picker.set_active_id("whatszan");
        picker.refresh_custom(app);
        picker
    }

    fn set_active_id(&self, id: &str) {
        for (choice, button) in &self.buttons {
            button.set_active(choice == id);
        }
    }

    /// Re-read the saved custom icon and use it as the "Kustom" thumbnail.
    fn refresh_custom(&self, app: &AppHandle) {
        let state = app.state::<AppState>();
        let (choice, path) = {
            let persist = state.persist.read().unwrap();
            (
                persist
                    .get_str(&format!("icon-choice-{}", self.kind), "whatszan")
                    .to_string(),
                persist
                    .get_str(&format!("custom_tray_{}", self.kind), "")
                    .to_string(),
            )
        };
        let pixbuf = if choice == "custom" && !path.is_empty() {
            gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&path, 32, 32, true).ok()
        } else {
            None
        };
        set_custom_thumbnail(&self.custom_image, pixbuf.as_ref());
    }
}

fn icon_image(bytes: &[u8], size: i32) -> gtk::Image {
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_read(std::io::Cursor::new(bytes.to_vec()))
        .ok()
        .and_then(|pixbuf| pixbuf.scale_simple(size, size, gtk::gdk_pixbuf::InterpType::Bilinear));
    let image = gtk::Image::new();
    if let Some(pixbuf) = pixbuf {
        image.set_from_pixbuf(Some(&pixbuf));
    }
    image.set_pixel_size(size);
    image
}

fn set_custom_thumbnail(image: &gtk::Image, pixbuf: Option<&gtk::gdk_pixbuf::Pixbuf>) {
    match pixbuf {
        Some(pixbuf) => {
            image.set_from_pixbuf(Some(pixbuf));
            image.set_pixel_size(32);
        }
        None => {
            image.set_from_icon_name(Some("list-add-symbolic"), gtk::IconSize::Button);
            image.set_pixel_size(24);
        }
    }
}

fn icon_field(
    app: &AppHandle,
    title: &str,
    desc: &str,
    picker: &IconPicker,
) -> (gtk::Box, gtk::Button) {
    let choose = gtk::Button::with_label(&tr(app, "settings_btn_browse", "Pilih Gambar"));
    choose.set_halign(gtk::Align::Start);
    let controls = gtk::Box::new(gtk::Orientation::Vertical, 6);
    controls.pack_start(&picker.container, false, false, 0);
    controls.pack_start(&choose, false, false, 0);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
    container.style_context().add_class("wz-card");
    let title_label = gtk::Label::new(None);
    title_label.set_text(title);
    title_label.set_xalign(0.0);
    title_label.style_context().add_class("wz-title");
    let desc_label = gtk::Label::new(None);
    desc_label.set_text(desc);
    desc_label.set_xalign(0.0);
    desc_label.set_line_wrap(true);
    desc_label.style_context().add_class("wz-desc");
    container.pack_start(&title_label, false, false, 0);
    container.pack_start(&desc_label, false, false, 0);
    container.pack_start(&controls, false, false, 2);
    (container, choose)
}

fn connect_browse(form: &Rc<Form>, button: &gtk::Button, entry: &gtk::Entry) {
    let app = form.app.clone();
    let window = form.window.clone();
    let entry = entry.clone();
    button.connect_clicked(move |_| {
        let Some(path) = with_ignore_blur(&app, || pick_folder(&window)) else {
            return;
        };
        entry.set_text(&path.display().to_string());
        window.present();
    });
}

fn connect_signals(form: &Rc<Form>, saver: &Saver) {
    for switch in [
        &form.auto_run,
        &form.esc_toggle,
        &form.close_to_tray,
        &form.blur_contacts,
        &form.blur_pp,
        &form.blur_messages,
        &form.blur_media,
        &form.blur_input,
        &form.anti_screencast,
        &form.incognito_noread,
        &form.incognito_notyping,
        &form.incognito_antidelete,
        &form.incognito_statussaver,
        &form.menu_bar,
        &form.menu_bar_auto_hide,
        &form.spellcheck,
        &form.low_resource_mode,
        &form.disable_gpu,
        &form.desktop_shortcut,
    ] {
        on_switch(switch, saver);
    }

    {
        let form = form.clone();
        let saver = saver.clone();
        let control = form.smart_download.clone();
        control.connect_active_notify(move |switch| {
            let active = switch.is_active();
            form.smart_paths.set_visible(active);
            if active {
                form.smart_paths.show_all();
            }
            saver.schedule();
        });
    }

    {
        let form = form.clone();
        let saver = saver.clone();
        let control = form.privacy_blur.clone();
        control.connect_active_notify(move |switch| {
            let active = switch.is_active();
            if active
                && !form.blur_contacts.is_active()
                && !form.blur_pp.is_active()
                && !form.blur_messages.is_active()
                && !form.blur_media.is_active()
                && !form.blur_input.is_active()
            {
                for toggle in [
                    &form.blur_contacts,
                    &form.blur_pp,
                    &form.blur_messages,
                    &form.blur_media,
                    &form.blur_input,
                ] {
                    toggle.set_active(true);
                }
            }
            form.blur_box.set_visible(active);
            if active {
                form.blur_box.show_all();
            }
            saver.schedule();
        });
    }

    for entry in [
        &form.notif_prefix,
        &form.download_path_image,
        &form.download_path_video,
        &form.download_path_audio,
        &form.download_path_document,
        &form.blur_specific,
        &form.user_agent,
        &form.custom_app_name,
    ] {
        on_entry(entry, saver);
    }

    on_combo(&form.app_language, saver);
    on_scale(&form.blur_amount, saver);
    on_spin(&form.unblur_delay, saver);

    {
        let form = form.clone();
        let saver = saver.clone();
        let control = form.dict_add.clone();
        control.connect_clicked(move |_| {
            let mut trigger = form.dict_trigger.text().trim().to_lowercase();
            let replacement = form.dict_replacement.text().trim().to_string();
            if trigger.is_empty() || replacement.is_empty() {
                return;
            }
            trigger = trigger.trim_start_matches('/').to_string();
            if trigger.is_empty() {
                return;
            }
            {
                let mut replacements = form.text_replacements.borrow_mut();
                replacements.insert(trigger, json!(replacement));
            }
            form.dict_trigger.set_text("");
            form.dict_replacement.set_text("");
            render_dict_list(&form, &saver);
            saver.save_now();
        });
    }

    {
        let form = form.clone();
        form.webstore.connect_clicked(|_| commands::open_webstore());
    }

    connect_icon_picker(
        form,
        "app",
        &form.icon_app,
        &form.icon_last_app,
        &form.icon_app_choose,
    );
    connect_icon_picker(
        form,
        "unread",
        &form.icon_unread,
        &form.icon_last_unread,
        &form.icon_unread_choose,
    );
    connect_icon_picker(
        form,
        "normal",
        &form.icon_normal,
        &form.icon_last_normal,
        &form.icon_normal_choose,
    );

    {
        let form = form.clone();
        let control = form.reset_branding.clone();
        control.connect_clicked(move |_| {
            commands::reset_branding_icons(&form.app);
            for (picker, last) in [
                (&form.icon_app, &form.icon_last_app),
                (&form.icon_unread, &form.icon_last_unread),
                (&form.icon_normal, &form.icon_last_normal),
            ] {
                form.updating.set(true);
                picker.set_active_id("whatszan");
                form.updating.set(false);
                picker.refresh_custom(&form.app);
                *last.borrow_mut() = "whatszan".into();
            }
            show_status(
                &form,
                "settings_toast_icon_reset",
                "Ikon berhasil di-reset ke bawaan.",
            );
        });
    }
}

fn on_switch(switch: &gtk::Switch, saver: &Saver) {
    let saver = saver.clone();
    switch.connect_active_notify(move |_| saver.schedule());
}

fn on_entry(entry: &gtk::Entry, saver: &Saver) {
    let saver = saver.clone();
    entry.connect_changed(move |_| saver.schedule());
}

fn on_combo(combo: &gtk::ComboBoxText, saver: &Saver) {
    let saver = saver.clone();
    combo.connect_changed(move |_| saver.schedule());
}

fn on_scale(scale: &gtk::Scale, saver: &Saver) {
    let saver = saver.clone();
    scale.connect_value_changed(move |_| saver.schedule());
}

fn on_spin(spin: &gtk::SpinButton, saver: &Saver) {
    let saver = saver.clone();
    spin.connect_value_changed(move |_| saver.schedule());
}

fn connect_icon_picker(
    form: &Rc<Form>,
    kind: &'static str,
    picker: &IconPicker,
    last: &Rc<RefCell<String>>,
    choose: &gtk::Button,
) {
    for (choice, button) in &picker.buttons {
        let form = form.clone();
        let picker = picker.clone();
        let last = last.clone();
        let choose = choose.clone();
        let choice = choice.clone();
        // `clicked` fires for both activation and deactivation, including
        // programmatic `set_active` calls. React to `toggled` only when the
        // tile actually becomes active, and suppress the updates performed
        // while normalising the toggle group; otherwise selecting one tile
        // deselects another, re-enters this handler and recurses until the
        // stack overflows.
        button.connect_toggled(move |button| {
            if form.updating.get() {
                return;
            }
            if !button.is_active() {
                // The tiles behave like a radio group: clicking the selected
                // tile must not leave every tile deselected.
                form.updating.set(true);
                button.set_active(true);
                form.updating.set(false);
                return;
            }
            form.updating.set(true);
            picker.set_active_id(&choice);
            form.updating.set(false);
            if choice == "custom" {
                choose.clicked();
                return;
            }
            match commands::upload_branding_path(&form.app, kind, &choice) {
                Ok(()) => {
                    picker.refresh_custom(&form.app);
                    *last.borrow_mut() = choice.clone();
                    show_status(
                        &form,
                        "settings_toast_icon_changed",
                        "Ikon berhasil diubah!",
                    );
                }
                Err(err) => {
                    tracing::warn!(%err, kind, "cannot change branding icon");
                    restore_choice(&form, &picker, &last.borrow());
                }
            }
        });
    }

    {
        let form = form.clone();
        let last = last.clone();
        let picker = picker.clone();
        choose.connect_clicked(move |_| {
            let previous = last.borrow().clone();
            let Some(path) = with_ignore_blur(&form.app, || pick_image(&form.window)) else {
                restore_choice(&form, &picker, &previous);
                return;
            };
            match commands::upload_branding_path(&form.app, kind, &path.display().to_string()) {
                Ok(()) => {
                    form.updating.set(true);
                    picker.set_active_id("custom");
                    form.updating.set(false);
                    picker.refresh_custom(&form.app);
                    *last.borrow_mut() = "custom".into();
                    show_status(
                        &form,
                        "settings_toast_custom_uploaded",
                        "Ikon kustom berhasil diunggah dan diterapkan!",
                    );
                }
                Err(err) => {
                    tracing::warn!(%err, kind, "cannot upload branding icon");
                    restore_choice(&form, &picker, &previous);
                    show_status(&form, "settings_toast_saved", &format!("Gagal: {err}"));
                }
            }
            form.window.present();
        });
    }
}

fn restore_choice(form: &Form, picker: &IconPicker, choice: &str) {
    form.updating.set(true);
    picker.set_active_id(choice);
    form.updating.set(false);
}

fn render_dict_list(form: &Rc<Form>, saver: &Saver) {
    for child in form.dict_list.children() {
        form.dict_list.remove(&child);
    }

    let entries: Vec<(String, String)> = form
        .text_replacements
        .borrow()
        .iter()
        .filter_map(|(trigger, value)| {
            value
                .as_str()
                .map(|replacement| (trigger.clone(), replacement.to_string()))
        })
        .collect();

    for (trigger, replacement) in entries {
        let row = gtk::ListBoxRow::new();
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        line.set_margin_top(4);
        line.set_margin_bottom(4);

        let text = gtk::Label::new(None);
        text.set_markup(&format!(
            "<b>/{}</b>  ➔  {}",
            gtk::glib::markup_escape_text(&trigger),
            gtk::glib::markup_escape_text(&replacement)
        ));
        text.set_xalign(0.0);
        text.set_line_wrap(true);
        text.set_hexpand(true);
        line.pack_start(&text, true, true, 0);

        let edit = gtk::Button::with_label("Edit");
        let delete = gtk::Button::with_label(&tr(&form.app, "settings_btn_delete", "Hapus"));
        line.pack_end(&delete, false, false, 0);
        line.pack_end(&edit, false, false, 0);

        {
            let form = form.clone();
            let trigger = trigger.clone();
            let replacement = replacement.clone();
            edit.connect_clicked(move |_| {
                form.dict_trigger.set_text(&format!("/{trigger}"));
                form.dict_replacement.set_text(&replacement);
                form.dict_replacement.grab_focus();
            });
        }

        {
            let form = form.clone();
            let saver = saver.clone();
            let trigger = trigger.clone();
            delete.connect_clicked(move |_| {
                form.text_replacements.borrow_mut().remove(&trigger);
                render_dict_list(&form, &saver);
                saver.save_now();
            });
        }

        row.add(&line);
        form.dict_list.add(&row);
        row.show_all();
    }
}

#[derive(Clone)]
struct Saver {
    form: Rc<Form>,
    timer: Rc<RefCell<Option<gtk::glib::SourceId>>>,
}

impl Saver {
    fn new(form: Rc<Form>) -> Self {
        Self {
            form,
            timer: Rc::new(RefCell::new(None)),
        }
    }

    fn schedule(&self) {
        if let Some(id) = self.timer.borrow_mut().take() {
            id.remove();
        }
        let this = self.clone();
        let id = gtk::glib::timeout_add_local(Duration::from_millis(350), move || {
            this.timer.borrow_mut().take();
            this.save_now();
            gtk::glib::ControlFlow::Break
        });
        *self.timer.borrow_mut() = Some(id);
    }

    fn save_now(&self) {
        let language = self
            .form
            .app_language
            .active_id()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "auto".into());
        let language_changed = language != *self.form.last_language.borrow();
        let settings = self.form.to_value();
        match commands::save_settings(&self.form.app, settings) {
            Ok(()) => {
                if language_changed {
                    *self.form.last_language.borrow_mut() = language;
                    // Labels are built from translations while the window is
                    // created. Rebuild once so a language change is visible
                    // immediately, matching the Electron settings page.
                    let app = self.form.app.clone();
                    SETTINGS_WINDOW.with(|slot| *slot.borrow_mut() = None);
                    self.form.window.close();
                    gtk::glib::timeout_add_local(Duration::from_millis(60), move || {
                        open(&app);
                        gtk::glib::ControlFlow::Break
                    });
                } else {
                    show_status(&self.form, "settings_toast_saved", "Tersimpan otomatis");
                }
            }
            Err(err) => {
                tracing::warn!(%err, "cannot save settings from GTK UI");
                show_status(&self.form, "settings_toast_saved", &format!("Gagal: {err}"));
            }
        }
    }
}

struct Form {
    app: AppHandle,
    window: gtk::Window,
    status: gtk::Label,
    app_language: gtk::ComboBoxText,
    auto_run: gtk::Switch,
    esc_toggle: gtk::Switch,
    close_to_tray: gtk::Switch,
    smart_download: gtk::Switch,
    smart_paths: gtk::Box,
    download_path_image: gtk::Entry,
    download_path_video: gtk::Entry,
    download_path_audio: gtk::Entry,
    download_path_document: gtk::Entry,
    notif_prefix: gtk::Entry,
    privacy_blur: gtk::Switch,
    blur_box: gtk::Box,
    blur_contacts: gtk::Switch,
    blur_pp: gtk::Switch,
    blur_messages: gtk::Switch,
    blur_media: gtk::Switch,
    blur_input: gtk::Switch,
    blur_amount: gtk::Scale,
    unblur_delay: gtk::SpinButton,
    blur_specific: gtk::Entry,
    anti_screencast: gtk::Switch,
    incognito_noread: gtk::Switch,
    incognito_notyping: gtk::Switch,
    incognito_antidelete: gtk::Switch,
    incognito_statussaver: gtk::Switch,
    dict_trigger: gtk::Entry,
    dict_replacement: gtk::Entry,
    dict_add: gtk::Button,
    dict_list: gtk::ListBox,
    text_replacements: RefCell<Map<String, Value>>,
    menu_bar: gtk::Switch,
    menu_bar_auto_hide: gtk::Switch,
    spellcheck: gtk::Switch,
    low_resource_mode: gtk::Switch,
    disable_gpu: gtk::Switch,
    user_agent: gtk::Entry,
    webstore: gtk::Button,
    custom_app_name: gtk::Entry,
    desktop_shortcut: gtk::Switch,
    icon_app: IconPicker,
    icon_unread: IconPicker,
    icon_normal: IconPicker,
    icon_last_app: Rc<RefCell<String>>,
    icon_last_unread: Rc<RefCell<String>>,
    icon_last_normal: Rc<RefCell<String>>,
    icon_app_choose: gtk::Button,
    icon_unread_choose: gtk::Button,
    icon_normal_choose: gtk::Button,
    reset_branding: gtk::Button,
    updating: Cell<bool>,
    sidebar_width: Cell<u64>,
    status_token: Cell<u64>,
    last_language: RefCell<String>,
}

impl Form {
    fn to_value(&self) -> Value {
        let user_agent = self.user_agent.text().to_string();
        let mut settings = Map::new();
        settings.insert(
            "app-language".into(),
            json!(self
                .app_language
                .active_id()
                .map(|id| id.to_string())
                .unwrap_or_else(|| "auto".into())),
        );
        settings.insert(
            "notification-prefix".into(),
            json!(self.notif_prefix.text().to_string()),
        );
        settings.insert("auto-run".into(), json!(self.auto_run.is_active()));
        // The switch is labelled "Close to Tray", while the stored key keeps
        // the Electron meaning (`quit-on-close`), so the value is inverted.
        settings.insert(
            "quit-on-close".into(),
            json!(quit_on_close_from_switch(self.close_to_tray.is_active())),
        );
        settings.insert(
            "esc-toggle-window".into(),
            json!(self.esc_toggle.is_active()),
        );
        settings.insert(
            "smart-download".into(),
            json!(self.smart_download.is_active()),
        );
        settings.insert(
            "download-path-image".into(),
            json!(self.download_path_image.text().to_string()),
        );
        settings.insert(
            "download-path-video".into(),
            json!(self.download_path_video.text().to_string()),
        );
        settings.insert(
            "download-path-audio".into(),
            json!(self.download_path_audio.text().to_string()),
        );
        settings.insert(
            "download-path-document".into(),
            json!(self.download_path_document.text().to_string()),
        );
        settings.insert("spellcheck".into(), json!(self.spellcheck.is_active()));
        settings.insert(
            "low-resource-mode".into(),
            json!(self.low_resource_mode.is_active()),
        );
        settings.insert("disable-gpu".into(), json!(self.disable_gpu.is_active()));
        settings.insert("menu-bar".into(), json!(self.menu_bar.is_active()));
        settings.insert(
            "menu-bar-auto-hide".into(),
            json!(self.menu_bar_auto_hide.is_active()),
        );
        settings.insert(
            "user-agent".into(),
            if user_agent.trim().is_empty() {
                Value::Null
            } else {
                json!(user_agent)
            },
        );
        settings.insert("privacy-blur".into(), json!(self.privacy_blur.is_active()));
        settings.insert(
            "anti-screencast".into(),
            json!(self.anti_screencast.is_active()),
        );
        settings.insert(
            "blur-contacts".into(),
            json!(self.blur_contacts.is_active()),
        );
        settings.insert("blur-pp".into(), json!(self.blur_pp.is_active()));
        settings.insert(
            "blur-messages".into(),
            json!(self.blur_messages.is_active()),
        );
        settings.insert("blur-media".into(), json!(self.blur_media.is_active()));
        settings.insert("blur-input".into(), json!(self.blur_input.is_active()));
        settings.insert(
            "blur-specific".into(),
            json!(self.blur_specific.text().to_string()),
        );
        settings.insert("blur-amount".into(), json!(self.blur_amount.value() as i64));
        settings.insert("unblur-delay".into(), json!(self.unblur_delay.value()));
        settings.insert(
            "text-replacements".into(),
            Value::Object(self.text_replacements.borrow().clone()),
        );
        settings.insert(
            "incognito-antidelete".into(),
            json!(self.incognito_antidelete.is_active()),
        );
        settings.insert(
            "incognito-statussaver".into(),
            json!(self.incognito_statussaver.is_active()),
        );
        settings.insert(
            "incognito-noread".into(),
            json!(self.incognito_noread.is_active()),
        );
        settings.insert(
            "incognito-notyping".into(),
            json!(self.incognito_notyping.is_active()),
        );
        settings.insert("sidebar-width".into(), json!(self.sidebar_width.get()));
        settings.insert(
            "custom-app-name".into(),
            json!(self.custom_app_name.text().to_string()),
        );
        settings.insert(
            "desktop-shortcut".into(),
            json!(self.desktop_shortcut.is_active()),
        );
        Value::Object(settings)
    }
}

/// The switch is labelled "Close to Tray", while `quit-on-close` is the
/// Electron-compatible storage key with the opposite meaning. Keep the
/// translation in one place so the UI cannot drift from the config again.
fn quit_on_close_from_switch(close_to_tray: bool) -> bool {
    !close_to_tray
}

fn close_to_tray_from_config(quit_on_close: bool) -> bool {
    !quit_on_close
}

fn load_values(form: &Rc<Form>, settings: &Value, _translations: &Map<String, Value>) {
    let language = get_str(settings, "app-language", "auto").to_string();
    form.app_language.set_active_id(Some(&language));
    *form.last_language.borrow_mut() = language;
    form.auto_run
        .set_active(get_bool(settings, "auto-run", false));
    form.esc_toggle
        .set_active(get_bool(settings, "esc-toggle-window", false));
    form.close_to_tray
        .set_active(close_to_tray_from_config(get_bool(
            settings,
            "quit-on-close",
            false,
        )));
    form.smart_download
        .set_active(get_bool(settings, "smart-download", true));
    form.notif_prefix
        .set_text(get_str(settings, "notification-prefix", ""));

    let defaults = commands::get_default_download_paths();
    for (key, entry) in [
        ("image", &form.download_path_image),
        ("video", &form.download_path_video),
        ("audio", &form.download_path_audio),
        ("document", &form.download_path_document),
    ] {
        let default = defaults
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let configured = get_str(settings, &format!("download-path-{key}"), "");
        entry.set_text(if configured.is_empty() {
            default.as_str()
        } else {
            configured
        });
        entry.set_placeholder_text(Some(&default));
    }

    let blur_on = get_bool(settings, "privacy-blur", false);
    form.privacy_blur.set_active(blur_on);
    form.blur_contacts
        .set_active(get_bool(settings, "blur-contacts", true));
    form.blur_pp.set_active(get_bool(settings, "blur-pp", true));
    form.blur_messages
        .set_active(get_bool(settings, "blur-messages", true));
    form.blur_media
        .set_active(get_bool(settings, "blur-media", true));
    form.blur_input
        .set_active(get_bool(settings, "blur-input", true));
    form.blur_amount
        .set_value(get_f64(settings, "blur-amount", 4.0));
    form.unblur_delay
        .set_value(get_f64(settings, "unblur-delay", 0.0));
    form.blur_specific
        .set_text(get_str(settings, "blur-specific", ""));
    form.anti_screencast
        .set_active(get_bool(settings, "anti-screencast", false));
    form.incognito_noread
        .set_active(get_bool(settings, "incognito-noread", false));
    form.incognito_notyping
        .set_active(get_bool(settings, "incognito-notyping", false));
    form.incognito_antidelete
        .set_active(get_bool(settings, "incognito-antidelete", false));
    form.incognito_statussaver
        .set_active(get_bool(settings, "incognito-statussaver", false));

    form.menu_bar
        .set_active(get_bool(settings, "menu-bar", true));
    form.menu_bar_auto_hide
        .set_active(get_bool(settings, "menu-bar-auto-hide", true));
    form.spellcheck
        .set_active(get_bool(settings, "spellcheck", true));
    form.low_resource_mode
        .set_active(get_bool(settings, "low-resource-mode", true));
    form.disable_gpu
        .set_active(get_bool(settings, "disable-gpu", false));
    form.user_agent
        .set_text(get_str(settings, "user-agent", ""));

    form.custom_app_name
        .set_text(get_str(settings, "custom-app-name", "WhatsZan"));
    form.desktop_shortcut
        .set_active(get_bool(settings, "desktop-shortcut", true));

    for (picker, last, key) in [
        (&form.icon_app, &form.icon_last_app, "icon-choice-app"),
        (
            &form.icon_unread,
            &form.icon_last_unread,
            "icon-choice-unread",
        ),
        (
            &form.icon_normal,
            &form.icon_last_normal,
            "icon-choice-normal",
        ),
    ] {
        let choice = get_str(settings, key, "whatszan").to_string();
        form.updating.set(true);
        picker.set_active_id(&choice);
        form.updating.set(false);
        picker.refresh_custom(&form.app);
        *last.borrow_mut() = choice;
    }

    let replacements = settings
        .get("text-replacements")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    *form.text_replacements.borrow_mut() = replacements;
    form.sidebar_width.set(
        settings
            .get("sidebar-width")
            .and_then(Value::as_u64)
            .unwrap_or(220),
    );

    form.smart_paths
        .set_visible(form.smart_download.is_active());
    if form.smart_download.is_active() {
        form.smart_paths.show_all();
    }
    form.blur_box.set_visible(blur_on);
    if blur_on {
        form.blur_box.show_all();
    }
}

fn get_bool(settings: &Value, key: &str, default: bool) -> bool {
    settings
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn get_str<'a>(settings: &'a Value, key: &str, default: &'a str) -> &'a str {
    settings.get(key).and_then(Value::as_str).unwrap_or(default)
}

fn get_f64(settings: &Value, key: &str, default: f64) -> f64 {
    settings.get(key).and_then(Value::as_f64).unwrap_or(default)
}

fn show_status(form: &Form, key: &str, fallback: &str) {
    let text = tr(&form.app, key, fallback);
    form.status.set_text(&text);
    if text.is_empty() {
        form.status.hide();
        return;
    }
    form.status.show();
    let token = form.status_token.get().wrapping_add(1);
    form.status_token.set(token);
    let label = form.status.clone();
    let current = form.status_token.clone();
    gtk::glib::timeout_add_seconds_local(2, move || {
        if current.get() == token {
            label.set_text("");
            label.hide();
        }
        gtk::glib::ControlFlow::Break
    });
}

fn tr(app: &AppHandle, key: &str, fallback: &str) -> String {
    let state = app.state::<AppState>();
    let translations = state.translations();
    t(&translations, key, fallback).to_string()
}

fn with_ignore_blur<T>(app: &AppHandle, action: impl FnOnce() -> T) -> T {
    let state = app.state::<AppState>();
    state.set_settings_ignore_blur(true);
    let result = action();
    state.set_settings_ignore_blur(false);
    result
}

fn pick_image(parent: &gtk::Window) -> Option<PathBuf> {
    let dialog = gtk::FileChooserDialog::builder()
        .title("Pilih Ikon")
        .transient_for(parent)
        .action(gtk::FileChooserAction::Open)
        .build();
    // Keep the chooser inside small laptop screens; GTK's default size can
    // exceed the display and push the action buttons off-screen.
    dialog.set_default_size(900, 600);
    dialog.set_position(gtk::WindowPosition::CenterOnParent);
    dialog.add_button("Batal", gtk::ResponseType::Cancel);
    dialog.add_button("Pilih", gtk::ResponseType::Accept);
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Gambar"));
    filter.add_mime_type("image/*");
    filter.add_pattern("*.ico");
    filter.add_pattern("*.png");
    filter.add_pattern("*.jpg");
    filter.add_pattern("*.jpeg");
    filter.add_pattern("*.webp");
    dialog.add_filter(filter);
    let response = dialog.run();
    let path = if response == gtk::ResponseType::Accept {
        dialog.filename()
    } else {
        None
    };
    dialog.close();
    path
}

fn pick_folder(parent: &gtk::Window) -> Option<PathBuf> {
    let dialog = gtk::FileChooserDialog::builder()
        .title("Pilih Folder")
        .transient_for(parent)
        .action(gtk::FileChooserAction::SelectFolder)
        .build();
    dialog.set_default_size(900, 600);
    dialog.set_position(gtk::WindowPosition::CenterOnParent);
    dialog.add_button("Batal", gtk::ResponseType::Cancel);
    dialog.add_button("Pilih", gtk::ResponseType::Accept);
    let response = dialog.run();
    let path = if response == gtk::ResponseType::Accept {
        dialog.filename()
    } else {
        None
    };
    dialog.close();
    path
}

fn install_css() {
    if CSS_INSTALLED.with(|installed| installed.replace(true)) {
        return;
    }

    // Mirrors the palette and spacing of `static/settings.html`: a white
    // header/sidebar around a warm background, cards for each setting, and
    // WhatsApp green for the active tab and primary actions.
    let provider = gtk::CssProvider::new();
    let css = r#"
        window.wz-window {
            background-color: #f8f5f3;
            color: #111b21;
            font-family: Inter, Cantarell, sans-serif;
        }
        window.wz-window.wz-dark {
            background-color: #161717;
            color: #e9edef;
        }

        .wz-window .wz-header {
            background-color: #ffffff;
            border-bottom: 1px solid #e9edef;
            padding: 14px 18px;
        }
        .wz-window .wz-header-icon {
            background-color: rgba(0, 168, 132, 0.10);
            border-radius: 999px;
        }
        .wz-window .wz-header-title {
            color: #111b21;
            font-size: 15px;
            font-weight: 600;
        }
        .wz-window .wz-header-subtitle {
            color: #667781;
            font-size: 12px;
        }

        .wz-window .wz-sidebar {
            background-color: #ffffff;
            border-right: 1px solid #e9edef;
            padding: 12px 10px;
        }
        .wz-window .wz-content,
        .wz-window .wz-stack,
        .wz-window scrolledwindow,
        .wz-window scrolledwindow viewport {
            background-color: #f8f5f3;
        }

        .wz-window button {
            background-color: #f8f5f3;
            background-image: none;
            border: 1px solid #e9edef;
            border-radius: 8px;
            box-shadow: none;
            padding: 6px 14px;
            color: #111b21;
            font-weight: 600;
        }
        .wz-window button label {
            color: #111b21;
        }
        .wz-window button:hover {
            background-color: #f5f6f6;
        }
        .wz-window button:active {
            background-color: #e9edef;
        }
        .wz-window button.wz-primary {
            background-color: #00a884;
            border-color: #00a884;
        }
        .wz-window button.wz-primary label {
            color: #ffffff;
        }
        .wz-window button.wz-primary:hover {
            background-color: #017f62;
            border-color: #017f62;
        }

        .wz-window button.wz-tab {
            background-color: transparent;
            border: none;
            box-shadow: none;
            outline-style: none;
            outline-width: 0;
            border-radius: 8px;
            padding: 10px 14px;
            color: #667781;
            font-size: 14px;
            font-weight: 500;
            min-height: 0;
        }
        .wz-window button.wz-tab:focus {
            outline-style: none;
            outline-width: 0;
            box-shadow: none;
        }
        .wz-window button.wz-tab label {
            color: #667781;
            font-weight: 500;
        }
        .wz-window button.wz-tab:hover {
            background-color: #f5f6f6;
        }
        .wz-window button.wz-tab:hover label {
            color: #111b21;
        }
        .wz-window button.wz-tab:checked {
            background-color: rgba(0, 168, 132, 0.10);
        }
        .wz-window button.wz-tab:checked label {
            color: #00a884;
            font-weight: 600;
        }
        .wz-window .wz-tab-icon {
            color: #667781;
        }
        .wz-window button.wz-tab:hover .wz-tab-icon {
            color: #111b21;
        }
        .wz-window button.wz-tab:checked .wz-tab-icon {
            color: #00a884;
        }

        .wz-window .wz-section {
            color: #111b21;
            font-size: 18px;
            font-weight: 600;
            margin-top: 4px;
            margin-bottom: 6px;
        }
        .wz-window .wz-card {
            background-color: #ffffff;
            border: 1px solid #e9edef;
            border-radius: 10px;
            padding: 14px 16px;
        }
        .wz-window .wz-card:hover {
            border-color: #00a884;
        }
        .wz-window .wz-title {
            color: #111b21;
            font-size: 14px;
            font-weight: 500;
        }
        .wz-window .wz-desc {
            color: #667781;
            font-size: 12.5px;
            margin-top: 1px;
        }

        .wz-window button.wz-icon-option {
            background-color: #f8f5f3;
            background-image: none;
            border: 2px solid transparent;
            border-radius: 8px;
            box-shadow: none;
            padding: 8px 10px;
            min-width: 58px;
            min-height: 58px;
        }
        .wz-window button.wz-icon-option:hover {
            background-color: #f5f6f6;
            border-color: #d3d9dc;
        }
        .wz-window button.wz-icon-option:checked {
            background-color: rgba(0, 168, 132, 0.10);
            border-color: #00a884;
        }
        .wz-window button.wz-icon-option label.wz-icon-caption {
            color: #111b21;
            font-size: 10px;
            font-weight: 600;
        }

        .wz-window entry {
            background-color: #f8f5f3;
            background-image: none;
            border: 1px solid #e9edef;
            border-radius: 8px;
            box-shadow: none;
            padding: 7px 10px;
            color: #111b21;
            min-width: 160px;
        }
        .wz-window entry:focus {
            border-color: #00a884;
        }

        .wz-window spinbutton {
            background-color: #f8f5f3;
            background-image: none;
            border: 1px solid #e9edef;
            border-radius: 8px;
            box-shadow: none;
            padding: 4px 8px;
            color: #111b21;
        }
        .wz-window spinbutton entry {
            background-color: transparent;
            border: none;
            box-shadow: none;
            padding: 0;
            min-width: 0;
        }
        .wz-window spinbutton button {
            background-color: transparent;
            border: none;
            box-shadow: none;
            padding: 0 4px;
        }

        .wz-window combobox button {
            background-color: #f8f5f3;
            border: 1px solid #e9edef;
            border-radius: 8px;
            padding: 5px 10px;
        }
        .wz-window combobox menu {
            background-color: #ffffff;
            border: 1px solid #e9edef;
            border-radius: 8px;
            padding: 4px;
        }
        .wz-window combobox menu menuitem {
            padding: 6px 10px;
            border-radius: 6px;
        }
        .wz-window combobox menu menuitem:hover {
            background-color: #f5f6f6;
        }

        .wz-window switch {
            background-color: #8696a0;
            background-image: none;
            border: none;
            box-shadow: none;
            border-radius: 999px;
            min-width: 46px;
            min-height: 26px;
        }
        .wz-window switch:checked {
            background-color: #00a884;
        }
        .wz-window switch slider {
            background-color: #ffffff;
            background-image: none;
            border: none;
            box-shadow: none;
            border-radius: 999px;
            min-width: 18px;
            min-height: 18px;
            margin: 4px;
        }

        .wz-window scale {
            color: #00a884;
        }
        .wz-window scale trough {
            background-color: #e9edef;
            border: none;
            box-shadow: none;
            border-radius: 999px;
            min-height: 6px;
        }
        .wz-window scale highlight {
            background-color: #00a884;
            border-radius: 999px;
        }
        .wz-window scale slider {
            background-color: #ffffff;
            background-image: none;
            border: 2px solid #00a884;
            box-shadow: none;
            border-radius: 999px;
            min-width: 14px;
            min-height: 14px;
        }

        .wz-window list.wz-dict-list {
            background-color: transparent;
        }
        .wz-window list.wz-dict-list row {
            background-color: #ffffff;
            border: 1px solid #e9edef;
            border-radius: 10px;
            padding: 10px 12px;
            margin-bottom: 6px;
        }
        .wz-window list.wz-dict-list row:hover {
            border-color: #00a884;
        }

        .wz-window .wz-status {
            background-color: #ffffff;
            border: 1px solid #e9edef;
            border-radius: 8px;
            padding: 8px 18px;
            color: #111b21;
            font-size: 13px;
            font-weight: 500;
        }

        .wz-window scrollbar {
            background-color: transparent;
            border: none;
        }
        .wz-window scrollbar slider {
            background-color: #d3d9dc;
            border-radius: 999px;
            min-width: 6px;
            min-height: 6px;
        }
        .wz-window scrollbar slider:hover {
            background-color: #b8c0c4;
        }

        .wz-window.wz-dark .wz-header {
            background-color: #1c1f1f;
            border-bottom-color: #2c2f30;
        }
        .wz-window.wz-dark .wz-header-icon {
            background-color: rgba(0, 168, 132, 0.12);
        }
        .wz-window.wz-dark .wz-header-title {
            color: #e9edef;
        }
        .wz-window.wz-dark .wz-header-subtitle {
            color: #8696a0;
        }
        .wz-window.wz-dark .wz-sidebar {
            background-color: #1c1f1f;
            border-right-color: #2c2f30;
        }
        .wz-window.wz-dark .wz-content,
        .wz-window.wz-dark .wz-stack,
        .wz-window.wz-dark scrolledwindow,
        .wz-window.wz-dark scrolledwindow viewport {
            background-color: #161717;
        }
        .wz-window.wz-dark button {
            background-color: #242627;
            border-color: #2c2f30;
            color: #e9edef;
        }
        .wz-window.wz-dark button label {
            color: #e9edef;
        }
        .wz-window.wz-dark button:hover {
            background-color: #2c2f30;
        }
        .wz-window.wz-dark button.wz-primary {
            background-color: #00a884;
            border-color: #00a884;
        }
        .wz-window.wz-dark button.wz-primary label {
            color: #ffffff;
        }
        .wz-window.wz-dark button.wz-tab {
            background-color: transparent;
        }
        .wz-window.wz-dark button.wz-tab label {
            color: #8696a0;
        }
        .wz-window.wz-dark button.wz-tab:hover {
            background-color: #242627;
        }
        .wz-window.wz-dark button.wz-tab:hover label {
            color: #e9edef;
        }
        .wz-window.wz-dark button.wz-tab:checked {
            background-color: rgba(0, 168, 132, 0.12);
        }
        .wz-window.wz-dark button.wz-tab:checked label {
            color: #00c49a;
        }
        .wz-window.wz-dark .wz-tab-icon {
            color: #8696a0;
        }
        .wz-window.wz-dark button.wz-tab:hover .wz-tab-icon {
            color: #e9edef;
        }
        .wz-window.wz-dark button.wz-tab:checked .wz-tab-icon {
            color: #00c49a;
        }
        .wz-window.wz-dark .wz-section {
            color: #e9edef;
        }
        .wz-window.wz-dark .wz-card {
            background-color: #242627;
            border-color: #2c2f30;
        }
        .wz-window.wz-dark .wz-title {
            color: #e9edef;
        }
        .wz-window.wz-dark .wz-desc {
            color: #8696a0;
        }
        .wz-window.wz-dark button.wz-icon-option {
            background-color: #1c1f1f;
            border-color: transparent;
        }
        .wz-window.wz-dark button.wz-icon-option:hover {
            background-color: #2c2f30;
            border-color: #3b4041;
        }
        .wz-window.wz-dark button.wz-icon-option:checked {
            background-color: rgba(0, 168, 132, 0.18);
            border-color: #00a884;
        }
        .wz-window.wz-dark button.wz-icon-option label.wz-icon-caption {
            color: #e9edef;
        }
        .wz-window.wz-dark entry {
            background-color: #1c1f1f;
            border-color: #2c2f30;
            color: #e9edef;
        }
        .wz-window.wz-dark spinbutton {
            background-color: #1c1f1f;
            border-color: #2c2f30;
            color: #e9edef;
        }
        .wz-window.wz-dark combobox button {
            background-color: #1c1f1f;
            border-color: #2c2f30;
        }
        .wz-window.wz-dark combobox menu {
            background-color: #242627;
            border-color: #2c2f30;
        }
        .wz-window.wz-dark combobox menu menuitem:hover {
            background-color: #2c2f30;
        }
        .wz-window.wz-dark switch {
            background-color: #3b4a54;
        }
        .wz-window.wz-dark switch:checked {
            background-color: #00a884;
        }
        .wz-window.wz-dark switch slider {
            background-color: #e9edef;
        }
        .wz-window.wz-dark scale trough {
            background-color: #2c2f30;
        }
        .wz-window.wz-dark scale slider {
            background-color: #1c1f1f;
        }
        .wz-window.wz-dark list.wz-dict-list row {
            background-color: #242627;
            border-color: #2c2f30;
        }
        .wz-window.wz-dark .wz-status {
            background-color: #1c1f1f;
            border-color: #2c2f30;
            color: #e9edef;
        }
        .wz-window.wz-dark scrollbar slider {
            background-color: #3b4a54;
        }
        .wz-window.wz-dark scrollbar slider:hover {
            background-color: #4a5a63;
        }
    "#;

    if let Err(err) = provider.load_from_data(css.as_bytes()) {
        tracing::warn!(%err, "cannot load settings stylesheet");
        CSS_INSTALLED.with(|installed| installed.set(false));
        return;
    }
    if let Some(screen) = gtk::gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_to_tray_is_the_inverse_of_quit_on_close() {
        assert!(close_to_tray_from_config(false));
        assert!(!close_to_tray_from_config(true));
        assert!(!quit_on_close_from_switch(true));
        assert!(quit_on_close_from_switch(false));
    }
}
