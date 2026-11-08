//! Windows shell integration: AppUserModelID, shortcuts, autostart, URL
//! protocol registration, and jump lists.
//!
//! This mirrors the Windows branches of `src/shortcuts.mjs` and `src/main.mjs`
//! while keeping the same `DesktopIntegration` surface used by
//! `commands.rs` on Linux.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use image::{
    codecs::ico::{IcoEncoder, IcoFrame},
    imageops::FilterType,
    ExtendedColorType,
};
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager, WebviewWindow};
use whatszan_core::translations::t;
use windows::{
    core::{Interface, HSTRING, PWSTR},
    Win32::{
        Foundation::{E_OUTOFMEMORY, PROPERTYKEY},
        Storage::EnhancedStorage::{
            PKEY_AppUserModel_ID, PKEY_AppUserModel_RelaunchCommand,
            PKEY_AppUserModel_RelaunchDisplayNameResource, PKEY_AppUserModel_RelaunchIconResource,
        },
        System::{
            Com::StructuredStorage::{PropVariantClear, PROPVARIANT},
            Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemAlloc, CoUninitialize, IPersistFile,
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
            },
            Variant::VT_LPWSTR,
        },
        UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
        UI::Shell::{
            Common::{IObjectArray, IObjectCollection},
            DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW,
            SetCurrentProcessExplicitAppUserModelID, ShellLink,
        },
    },
};
use winreg::{enums::HKEY_CURRENT_USER, RegKey};

use crate::{app_state::AppState, assets, notifications_common};

/// Stable AppUserModelID shared by the process, shortcuts, jump lists, and
/// toast notifications. Keeping it constant prevents taskbar duplication when
/// the display name is customized.
pub const APP_ID: &str = "org.uznbt.whatszan";

const SHORTCUT_DESCRIPTION: &str = "WhatsZan Desktop Client";

/// Distinguishes concurrent icon encodes (tray, jump list, shortcut refresh).
static ICON_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Set the process-wide AUMID before the first window exists.
pub fn set_process_app_id() {
    let app_id = HSTRING::from(APP_ID);
    if let Err(err) = unsafe { SetCurrentProcessExplicitAppUserModelID(&app_id) } {
        tracing::warn!(%err, "cannot set AppUserModelID");
    }
}

/// Register the app identity used by the taskbar, Start Menu, jump lists, and
/// WinRT toasts. Called once during setup, before the main window is built.
pub fn register_app_identity(app: &AppHandle) {
    set_process_app_id();

    let state = app.state::<AppState>();
    let name = state.custom_app_name();
    let icon = resolve_app_icon(&state).ok();

    // The Start Menu shortcut doubles as the AppUserModelID registration that
    // Windows requires for toasts and jump lists, so it is kept even when the
    // user hides the desktop shortcut.
    if let Some(icon) = icon.as_deref() {
        if let Err(err) = ensure_start_menu_shortcut(&name, icon) {
            tracing::warn!(%err, "cannot create Start Menu shortcut");
        }
    }

    if let Some(window) = state.main_window() {
        apply_window_identity(&window);
    }
}

/// Attach the AUMID and relaunch metadata to a top-level window. Windows uses
/// these properties to group taskbar buttons and to relaunch the correct app
/// from a toast or jump list.
pub fn apply_window_identity(window: &WebviewWindow) {
    let app = window.app_handle();
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let name = state.custom_app_name();
    let icon = resolve_app_icon(&state).ok();
    let Ok(hwnd) = window.hwnd() else {
        return;
    };

    let store: IPropertyStore = match unsafe { SHGetPropertyStoreForWindow(hwnd) } {
        Ok(store) => store,
        Err(err) => {
            tracing::debug!(%err, "cannot open window property store");
            return;
        }
    };

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("whatszan.exe"));
    let _ = set_prop_string(&store, &PKEY_AppUserModel_ID, APP_ID);
    let _ = set_prop_string(
        &store,
        &PKEY_AppUserModel_RelaunchCommand,
        &format!("\"{}\"", exe.display()),
    );
    let _ = set_prop_string(
        &store,
        &PKEY_AppUserModel_RelaunchDisplayNameResource,
        &name,
    );
    if let Some(icon) = icon {
        let _ = set_prop_string(
            &store,
            &PKEY_AppUserModel_RelaunchIconResource,
            &format!("\"{}\",0", icon.display()),
        );
    }
    let _ = unsafe { store.Commit() };
}

/// Resolve the current branding artwork to a cached multi-size `.ico` file.
///
/// Windows shortcuts and the shell icon cache require an ICO (or an executable
/// with icon resources); the settings UI stores ordinary PNG/JPEG artwork, so
/// the conversion happens here.
pub fn resolve_app_icon(state: &AppState) -> Result<PathBuf, String> {
    let mut bytes = notifications_common::branding_bytes(state);
    let mut image = match image::load_from_memory(&bytes) {
        Ok(image) => image,
        Err(err) => {
            tracing::warn!(%err, "cannot decode branded app icon; using default");
            bytes = assets::APP_ICON_PNG.to_vec();
            image::load_from_memory(&bytes).map_err(|err| format!("embedded app.png: {err}"))?
        }
    };

    let destination = state.user_data.join(format!(
        "app-icon-{:016x}.ico",
        notifications_common::hash(&bytes)
    ));

    if !destination.is_file() {
        fs::create_dir_all(&state.user_data)
            .map_err(|err| format!("cannot create {}: {err}", state.user_data.display()))?;
        image = image.resize(256, 256, FilterType::Lanczos3);
        encode_ico(&image, &destination)?;
    }

    if let Ok(entries) = fs::read_dir(&state.user_data) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path == destination {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("app-icon-") && name.ends_with(".ico") {
                let _ = fs::remove_file(path);
            }
        }
    }

    Ok(destination)
}

fn encode_ico(image: &image::DynamicImage, destination: &Path) -> Result<(), String> {
    let mut frames = Vec::new();
    for size in [16u32, 24, 32, 48, 64, 128, 256] {
        let resized = image.resize(size, size, FilterType::Lanczos3).to_rgba8();
        let (width, height) = resized.dimensions();
        let frame = IcoFrame::as_png(resized.as_raw(), width, height, ExtendedColorType::Rgba8)
            .map_err(|err| format!("cannot encode {size}px icon: {err}"))?;
        frames.push(frame);
    }

    let temp = destination.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        ICON_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<(), String> {
        let file = fs::File::create(&temp)
            .map_err(|err| format!("cannot create {}: {err}", temp.display()))?;
        IcoEncoder::new(file)
            .encode_images(&frames)
            .map_err(|err| format!("cannot encode {}: {err}", destination.display()))?;
        let _ = fs::remove_file(destination);
        fs::rename(&temp, destination)
            .map_err(|err| format!("cannot save {}: {err}", destination.display()))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Paths needed by the shell integration. The fields keep the Linux API shape
/// so `commands.rs` can construct both implementations the same way.
#[derive(Debug, Clone)]
pub struct DesktopIntegration {
    pub desktop_dir: PathBuf,
    pub exe: PathBuf,
}

impl DesktopIntegration {
    pub fn new(desktop_dir: impl Into<PathBuf>, exe: impl Into<PathBuf>) -> Self {
        Self {
            desktop_dir: desktop_dir.into(),
            exe: exe.into(),
        }
    }

    /// Create or remove the Startup folder shortcut.
    pub fn apply_auto_run(&self, auto_run: bool, _startup_run: bool) -> io::Result<()> {
        let startup = startup_dir()?;
        fs::create_dir_all(&startup)?;
        let shortcut = startup.join("WhatsZan.lnk");

        if auto_run {
            if !shortcut.exists() {
                let cwd = self.exe.parent().unwrap_or_else(|| Path::new("."));
                create_shortcut(
                    &shortcut,
                    &self.exe,
                    "--hide",
                    cwd,
                    &self.exe,
                    "Start WhatsZan hidden on login",
                    APP_ID,
                    "WhatsZan",
                )?;
            }
        } else {
            // Electron always removes the Startup shortcut when auto-run is
            // disabled, including during a startup/update pass.
            let _ = fs::remove_file(shortcut);
        }

        Ok(())
    }

    /// Create Desktop, Start Menu, and Send To shortcuts.
    pub fn apply_desktop_shortcut(
        &self,
        create: bool,
        old_name: &str,
        new_name: &str,
        icon_location: &str,
        translations: &Map<String, Value>,
    ) -> io::Result<()> {
        let old_name = if old_name.trim().is_empty() {
            "WhatsZan"
        } else {
            old_name.trim()
        };
        let new_name = if new_name.trim().is_empty() {
            "WhatsZan"
        } else {
            new_name.trim()
        };

        let start_menu = start_menu_programs_dir()?;
        let send_to = send_to_dir()?;
        let icon = Path::new(icon_location);
        let cwd = self.exe.parent().unwrap_or_else(|| Path::new("."));

        let old_paths = shortcut_paths(
            &self.desktop_dir,
            &start_menu,
            &send_to,
            old_name,
            translations,
        );
        for path in old_paths {
            let _ = fs::remove_file(path);
        }

        if new_name != "WhatsZan" {
            for path in shortcut_paths(
                &self.desktop_dir,
                &start_menu,
                &send_to,
                "WhatsZan",
                translations,
            ) {
                let _ = fs::remove_file(path);
            }
        }

        let new_paths = shortcut_paths(
            &self.desktop_dir,
            &start_menu,
            &send_to,
            new_name,
            translations,
        );

        if !create {
            // Keep the Start Menu entry: it registers the AppUserModelID used
            // by toasts and jump lists. Only the desktop/Send To entries are
            // removed when the user turns the shortcut setting off.
            for path in new_paths.iter() {
                if path.parent() == Some(start_menu.as_path()) {
                    continue;
                }
                let _ = fs::remove_file(path);
            }
            if let Err(err) = create_shortcut(
                &start_menu.join(format!("{new_name}.lnk")),
                &self.exe,
                "",
                cwd,
                icon,
                SHORTCUT_DESCRIPTION,
                APP_ID,
                new_name,
            ) {
                tracing::warn!(%err, "cannot refresh Start Menu shortcut");
            }
            return Ok(());
        }

        fs::create_dir_all(&self.desktop_dir)?;
        fs::create_dir_all(&start_menu)?;
        fs::create_dir_all(&send_to)?;

        let t_media = t(translations, "share_media", "Bagikan Media");
        let t_doc = t(translations, "share_document", "Bagikan Dokumen");

        let entries = [
            (
                self.desktop_dir.join(format!("{new_name}.lnk")),
                "",
                format!("{new_name} Desktop Shortcut"),
            ),
            (
                start_menu.join(format!("{new_name}.lnk")),
                "",
                format!("{new_name} Start Menu Shortcut"),
            ),
            (
                send_to.join(format!("{new_name} ({t_media}).lnk")),
                "--share-media",
                format!("{new_name} ({t_media})"),
            ),
            (
                send_to.join(format!("{new_name} ({t_doc}).lnk")),
                "--share-document",
                format!("{new_name} ({t_doc})"),
            ),
        ];

        for (path, args, description) in entries {
            if let Err(err) = create_shortcut(
                &path,
                &self.exe,
                args,
                cwd,
                icon,
                &description,
                APP_ID,
                new_name,
            ) {
                tracing::warn!(path = %path.display(), %err, "cannot create shortcut");
            }
        }

        Ok(())
    }

    /// Register this executable as the `whatsapp://` handler under HKCU.
    pub fn register_url_handler(&self) -> io::Result<()> {
        let (key, _) =
            RegKey::predef(HKEY_CURRENT_USER).create_subkey("Software\\Classes\\whatsapp")?;
        key.set_value("", &"URL:WhatsApp Protocol")?;
        key.set_value("URL Protocol", &"")?;
        let (command, _) = key.create_subkey("shell\\open\\command")?;
        command.set_value("", &format!("\"{}\" \"%1\"", self.exe.display()))?;
        Ok(())
    }
}

/// Make sure the shortcuts for the current branding exist. Called on every
/// startup after an upgrade check, matching the Linux repair path.
pub fn ensure_shortcuts(app: &AppHandle) {
    let state = app.state::<AppState>();
    let integration = desktop_integration();

    // Electron checks `isDefaultProtocolClient` on every launch. Writing the
    // registry key again is cheap and repairs a handler removed by another
    // app or by a profile cleanup.
    let register_scheme = state
        .config
        .read()
        .unwrap()
        .get_bool("register-url-scheme", true);
    if register_scheme {
        if let Err(err) = integration.register_url_handler() {
            tracing::debug!(%err, "cannot register whatsapp:// URL handler");
        }
    }

    let name = state.custom_app_name();
    let create = state
        .persist
        .read()
        .unwrap()
        .get_bool("desktop-shortcut", true);
    let translations = state.translations();
    let Ok(icon) = resolve_app_icon(&state) else {
        tracing::warn!("cannot prepare Windows app icon for shortcuts");
        return;
    };
    if let Err(err) = integration.apply_desktop_shortcut(
        create,
        &name,
        &name,
        &icon.display().to_string(),
        &translations,
    ) {
        tracing::warn!(%err, "cannot refresh Windows shortcuts");
    }
}

fn ensure_start_menu_shortcut(name: &str, icon: &Path) -> io::Result<()> {
    let start_menu = start_menu_programs_dir()?;
    fs::create_dir_all(&start_menu)?;
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("whatszan.exe"));
    let cwd = exe.parent().unwrap_or_else(|| Path::new("."));
    create_shortcut(
        &start_menu.join(format!("{name}.lnk")),
        &exe,
        "",
        cwd,
        icon,
        SHORTCUT_DESCRIPTION,
        APP_ID,
        name,
    )
}

fn shortcut_paths(
    desktop: &Path,
    start_menu: &Path,
    send_to: &Path,
    name: &str,
    translations: &Map<String, Value>,
) -> Vec<PathBuf> {
    let t_media = t(translations, "share_media", "Bagikan Media");
    let t_doc = t(translations, "share_document", "Bagikan Dokumen");
    vec![
        desktop.join(format!("{name}.lnk")),
        start_menu.join(format!("{name}.lnk")),
        send_to.join(format!("{name}.lnk")),
        send_to.join(format!("{name} ({t_media}).lnk")),
        send_to.join(format!("{name} ({t_doc}).lnk")),
    ]
}

/// Build and persist one `.lnk` file.
fn create_shortcut(
    path: &Path,
    target: &Path,
    args: &str,
    cwd: &Path,
    icon: &Path,
    description: &str,
    aumid: &str,
    title: &str,
) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_shortcut(path, target, args, cwd, icon, description, aumid, title)
        .map_err(|err| io::Error::other(format!("{err}")))
}

fn write_shortcut(
    path: &Path,
    target: &Path,
    args: &str,
    cwd: &Path,
    icon: &Path,
    description: &str,
    aumid: &str,
    title: &str,
) -> windows::core::Result<()> {
    let _com = ComGuard::new();
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };

    unsafe {
        link.SetPath(&HSTRING::from(path_text(target)))?;
        if !args.is_empty() {
            link.SetArguments(&HSTRING::from(args))?;
        }
        link.SetWorkingDirectory(&HSTRING::from(path_text(cwd)))?;
        link.SetDescription(&HSTRING::from(description))?;
        link.SetIconLocation(&HSTRING::from(path_text(icon)), 0)?;

        let store: IPropertyStore = link.cast()?;
        set_prop_string(&store, &PKEY_AppUserModel_ID, aumid)?;
        set_prop_string(
            &store,
            &PKEY_AppUserModel_RelaunchDisplayNameResource,
            title,
        )?;
        store.Commit()?;

        let persist: IPersistFile = link.cast()?;
        persist.Save(&HSTRING::from(path_text(path)), true)?;
    }

    Ok(())
}

/// Update the recent-chat jump list. Called from the renderer whenever the
/// visible chat list changes; a failed update is harmless.
pub fn update_recent_chats(app: &AppHandle, chats: &[String]) {
    if let Err(err) = try_update_recent_chats(app, chats) {
        tracing::warn!(%err, "cannot update jump list");
    }
}

fn try_update_recent_chats(app: &AppHandle, chats: &[String]) -> windows::core::Result<()> {
    let _com = ComGuard::new();
    let state = app.state::<AppState>();
    let translations = state.translations();
    let icon = resolve_app_icon(&state).ok();
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("whatszan.exe"));
    let cwd = exe.parent().unwrap_or_else(|| Path::new("."));
    let icon = icon.as_deref().unwrap_or(&exe);

    let list: ICustomDestinationList =
        unsafe { CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)? };
    unsafe {
        list.SetAppID(&HSTRING::from(APP_ID))?;
    }

    let mut removed_slots = 0u32;
    let _removed: IObjectArray = unsafe { list.BeginList(&mut removed_slots)? };

    let recent: IObjectCollection =
        unsafe { CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)? };
    let mut recent_count = 0;
    for chat in chats.iter().take(5) {
        let args = format!("--open-chat=\"{chat}\"");
        let link = jump_link(
            &exe,
            &args,
            cwd,
            icon,
            &format!("Buka obrolan dengan {chat}"),
            chat,
        )?;
        unsafe { recent.AddObject(&link.cast::<windows::core::IUnknown>()?)? };
        recent_count += 1;
    }
    if recent_count > 0 {
        let array: IObjectArray = recent.cast()?;
        let category = t(&translations, "recent_chats", "Recent Chats");
        unsafe { list.AppendCategory(&HSTRING::from(category), &array)? };
    }

    let tasks: IObjectCollection =
        unsafe { CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)? };
    let new_chat = t(&translations, "new_chat", "New Chat");
    let new_chat_desc = t(&translations, "new_chat_desc", "Start a new chat");
    let new_call = t(&translations, "new_call", "New Call");
    for (title, description, args) in [
        (new_chat, new_chat_desc, "--action=\"new-chat\""),
        (new_call, "Mulai panggilan baru", "--action=\"new-call\""),
    ] {
        let link = jump_link(&exe, args, cwd, icon, description, title)?;
        unsafe { tasks.AddObject(&link.cast::<windows::core::IUnknown>()?)? };
    }
    let tasks: IObjectArray = tasks.cast()?;
    unsafe { list.AddUserTasks(&tasks)? };
    unsafe { list.CommitList()? };

    Ok(())
}

fn jump_link(
    target: &Path,
    args: &str,
    cwd: &Path,
    icon: &Path,
    description: &str,
    title: &str,
) -> windows::core::Result<IShellLinkW> {
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };
    unsafe {
        link.SetPath(&HSTRING::from(path_text(target)))?;
        link.SetArguments(&HSTRING::from(args))?;
        link.SetWorkingDirectory(&HSTRING::from(path_text(cwd)))?;
        link.SetDescription(&HSTRING::from(title))?;
        link.SetIconLocation(&HSTRING::from(path_text(icon)), 0)?;
    }
    let _ = description;
    Ok(link)
}

/// Build the Windows integration object from the current executable.
pub fn desktop_integration() -> DesktopIntegration {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let desktop = dirs::desktop_dir().unwrap_or_else(|| home.join("Desktop"));
    DesktopIntegration::new(desktop, stable_exe_path())
}

pub fn stable_exe_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("whatszan.exe"))
}

fn start_menu_programs_dir() -> io::Result<PathBuf> {
    Ok(appdata()?.join("Microsoft\\Windows\\Start Menu\\Programs"))
}

fn startup_dir() -> io::Result<PathBuf> {
    Ok(start_menu_programs_dir()?.join("Startup"))
}

fn send_to_dir() -> io::Result<PathBuf> {
    Ok(appdata()?.join("Microsoft\\Windows\\SendTo"))
}

fn appdata() -> io::Result<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "APPDATA is not set"))
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn set_prop_string(
    store: &IPropertyStore,
    key: &PROPERTYKEY,
    value: &str,
) -> windows::core::Result<()> {
    let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let memory = unsafe { CoTaskMemAlloc(wide.len() * std::mem::size_of::<u16>()) } as *mut u16;
    if memory.is_null() {
        return Err(windows::core::Error::from_hresult(E_OUTOFMEMORY));
    }

    unsafe {
        std::ptr::copy_nonoverlapping(wide.as_ptr(), memory, wide.len());
        let mut prop = PROPVARIANT::default();
        (*prop.Anonymous.Anonymous).vt = VT_LPWSTR;
        (*prop.Anonymous.Anonymous).Anonymous.pwszVal = PWSTR(memory);
        let result = store.SetValue(key, &prop);
        let _ = PropVariantClear(&mut prop);
        result
    }
}

struct ComGuard {
    uninitialize: bool,
}

impl ComGuard {
    fn new() -> Self {
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        Self {
            uninitialize: result.is_ok(),
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.uninitialize {
            unsafe { CoUninitialize() };
        }
    }
}
