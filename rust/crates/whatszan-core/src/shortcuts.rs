//! Linux autostart and `.desktop` integration, ported from `src/shortcuts.mjs`.
//!
//! The Electron version also has Windows/macOS branches. The Rust client is
//! Linux-first; those platforms are intentionally no-ops here and can be
//! implemented behind `cfg` later without touching this API.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use serde_json::{Map, Value};

use crate::translations::t;

/// Autostart entry name.
const AUTOSTART_FILENAME: &str = "WhatsZan.desktop";
/// Older autostart entry name kept for cleanup.
const LEGACY_AUTOSTART_FILENAME: &str = "whatszan.desktop";
/// Application-menu entry used before entries were named after the app id.
/// The app rewrites it as `<app-id>.desktop` and removes this duplicate.
const LEGACY_APP_DESKTOP_FILENAME: &str = "WhatsZan.desktop";

/// Paths needed to create user-level desktop integration files.
#[derive(Debug, Clone)]
pub struct DesktopIntegration {
    pub home: PathBuf,
    pub desktop_dir: PathBuf,
    pub exe: PathBuf,
}

impl DesktopIntegration {
    pub fn new(
        home: impl Into<PathBuf>,
        desktop_dir: impl Into<PathBuf>,
        exe: impl Into<PathBuf>,
    ) -> Self {
        Self {
            home: home.into(),
            desktop_dir: desktop_dir.into(),
            exe: exe.into(),
        }
    }

    fn applications_dir(&self) -> PathBuf {
        self.home.join(".local/share/applications")
    }

    /// Wayland app id (and GTK program name) of the running binary.
    pub fn app_id(&self) -> String {
        let fallback = "whatszan".to_string();
        let Some(name) = self.exe.file_name() else {
            return fallback;
        };
        let name = name.to_string_lossy();
        let name = name.strip_suffix(".exe").unwrap_or(&name).to_string();
        if name.is_empty() {
            fallback
        } else {
            name
        }
    }

    /// User-level desktop entry matching [`Self::app_id`]. It overrides the
    /// package entry so docks and launchers pick up the current branding.
    pub fn app_desktop_path(&self) -> PathBuf {
        self.applications_dir()
            .join(format!("{}.desktop", self.app_id()))
    }

    fn autostart_dir(&self) -> PathBuf {
        self.home.join(".config/autostart")
    }

    fn script_dirs(&self) -> [PathBuf; 3] {
        [
            self.home.join(".local/share/nautilus/scripts"),
            self.home.join(".local/share/nemo/scripts"),
            self.home.join(".config/caja/scripts"),
        ]
    }

    /// Create or remove `~/.config/autostart/WhatsZan.desktop`.
    ///
    /// `startup_run` mirrors the Electron behaviour: during a normal startup
    /// the app never deletes a file it did not create. Only an explicit
    /// user toggle (`auto_run = false`, `startup_run = false`) removes it.
    pub fn apply_auto_run(&self, auto_run: bool, startup_run: bool) -> std::io::Result<()> {
        let autostart_dir = self.autostart_dir();
        let desktop_file = autostart_dir.join(AUTOSTART_FILENAME);
        let legacy_file = autostart_dir.join(LEGACY_AUTOSTART_FILENAME);

        fs::create_dir_all(&autostart_dir)?;
        let _ = fs::remove_file(&legacy_file);

        // An autostart entry must never appear in the launcher. Only the
        // NoDisplay/Hidden keys are touched; a user-supplied Exec stays intact.
        if let Ok(current) = fs::read_to_string(&desktop_file) {
            let updated = hide_from_launcher(&current);
            if updated != current {
                fs::write(&desktop_file, updated)?;
            }
        }

        if auto_run {
            if !desktop_file.exists() {
                let entry = format!(
                    "[Desktop Entry]\nType=Application\nExec=\"{}\" --hide\nNoDisplay=true\nHidden=true\nX-GNOME-Autostart-enabled=true\nName=WhatsZan\nComment=Start WhatsZan hidden on login\n",
                    self.exe.display()
                );
                fs::write(&desktop_file, entry)?;
            }
        } else if !startup_run {
            let _ = fs::remove_file(&desktop_file);
        }

        Ok(())
    }

    /// Write the launcher entry and (optionally) the desktop shortcut plus
    /// file-manager "Share with WhatsZan" actions.
    pub fn apply_desktop_shortcut(
        &self,
        create: bool,
        old_name: &str,
        new_name: &str,
        icon_location: &str,
        translations: &Map<String, Value>,
    ) -> std::io::Result<()> {
        let applications = self.applications_dir();
        let old_desktop = self.desktop_dir.join(format!("{old_name}.desktop"));
        let new_desktop = self.desktop_dir.join(format!("{new_name}.desktop"));
        let app_desktop = self.app_desktop_path();
        let legacy_app_desktop = applications.join(LEGACY_APP_DESKTOP_FILENAME);

        let old_name = if old_name.is_empty() {
            "WhatsZan"
        } else {
            old_name
        };
        let new_name = if new_name.is_empty() {
            "WhatsZan"
        } else {
            new_name
        };

        let _ = fs::remove_file(&old_desktop);
        self.clean_scripts(old_name);

        if new_name != "WhatsZan" {
            let _ = fs::remove_file(self.desktop_dir.join("WhatsZan.desktop"));
            self.clean_scripts("WhatsZan");
        }

        if !create {
            let _ = fs::remove_file(&new_desktop);
            let _ = fs::remove_file(&legacy_app_desktop);
            let _ = fs::remove_file(
                self.home
                    .join(".local/share/kio/servicemenus/whatszan-share.desktop"),
            );
            let _ = fs::remove_file(
                self.home
                    .join(".local/share/kservices5/ServiceMenus/whatszan-share.desktop"),
            );
            self.clean_scripts(new_name);
        }

        // Task switchers and docks look the app up by its Wayland app id,
        // which is the executable name. The generated entry must use the same
        // id (not the customizable display name) so the branded icon is found.
        let app_id = self.app_id();
        let desktop_entry = format!(
            "[Desktop Entry]\nName={new_name}\nExec=\"{}\" %U\nTerminal=false\nType=Application\nIcon={icon_location}\nStartupWMClass={app_id}\nComment=WhatsZan Desktop Client\nCategories=Network;Chat;InstantMessaging;\nMimeType=x-scheme-handler/whatsapp;",
            self.exe.display()
        );

        // Preserve a user-supplied wrapper in an existing Exec line when its
        // target still exists (custom launchers, distro wrappers, etc.).
        let mut final_entry = desktop_entry.clone();
        if let Ok(current) = fs::read_to_string(&app_desktop) {
            if let Some(existing_exec) = current
                .lines()
                .find(|line| line.starts_with("Exec="))
                .filter(|line| is_exec_usable(line))
            {
                final_entry = replace_exec_line(&desktop_entry, existing_exec);
            }
        }

        fs::create_dir_all(&applications)?;
        write_atomic(&app_desktop, final_entry.as_bytes())?;
        // Older builds always wrote `WhatsZan.desktop`, which made launchers
        // list the app twice next to the app-id entry. Keep only one entry.
        if legacy_app_desktop != app_desktop {
            let _ = fs::remove_file(&legacy_app_desktop);
        }

        if create {
            fs::create_dir_all(&self.desktop_dir)?;
            write_atomic(&new_desktop, desktop_entry.as_bytes())?;
            set_executable(&new_desktop);

            let t_media = t(translations, "share_media", "Bagikan Media");
            let t_doc = t(translations, "share_document", "Bagikan Dokumen");

            let dolphin_entry = format!(
                "[Desktop Entry]\nType=Service\nServiceTypes=KonqPopupMenu/Plugin\nMimeType=all/all;\nActions=ShareDocument;ShareMedia;\nX-KDE-Priority=TopLevel\n\n[Desktop Action ShareDocument]\nName={new_name} ({t_doc})\nIcon={icon_location}\nExec=\"{}\" --share-document %F\n\n[Desktop Action ShareMedia]\nName={new_name} ({t_media})\nIcon={icon_location}\nExec=\"{}\" --share-media %F\n",
                self.exe.display(),
                self.exe.display()
            );

            for dir in [
                self.home.join(".local/share/kio/servicemenus"),
                self.home.join(".local/share/kservices5/ServiceMenus"),
            ] {
                if fs::create_dir_all(&dir).is_ok() {
                    let _ = fs::write(dir.join("whatszan-share.desktop"), &dolphin_entry);
                }
            }

            let script_doc = format!(
                "#!/bin/bash\n\"{}\" --share-document \"$@\"\n",
                self.exe.display()
            );
            let script_media = format!(
                "#!/bin/bash\n\"{}\" --share-media \"$@\"\n",
                self.exe.display()
            );

            for dir in self.script_dirs() {
                if fs::create_dir_all(&dir).is_err() {
                    continue;
                }
                let doc_path = dir.join(format!("{new_name} ({t_doc})"));
                let media_path = dir.join(format!("{new_name} ({t_media})"));
                let _ = fs::write(&doc_path, &script_doc);
                let _ = fs::write(&media_path, &script_media);
                set_executable(&doc_path);
                set_executable(&media_path);
            }
        }

        // Refresh the launcher cache. Failure is harmless (the file is still
        // read on the next login) and `update-desktop-database` may be absent.
        let _ = run_with_timeout(
            Command::new("update-desktop-database").arg(&applications),
            Duration::from_secs(5),
        );

        Ok(())
    }

    /// Make this application the handler for `whatsapp://` links, matching
    /// Electron's `app.setAsDefaultProtocolClient("whatsapp")`.
    ///
    /// The `.desktop` file written by [`Self::apply_desktop_shortcut`] already
    /// advertises `x-scheme-handler/whatsapp`; `xdg-mime` tells the desktop
    /// environment to actually use it.
    pub fn register_url_handler(&self) -> std::io::Result<()> {
        let desktop_file = format!("{}.desktop", self.app_id());
        let status = run_with_timeout(
            Command::new("xdg-mime")
                .arg("default")
                .arg(desktop_file)
                .arg("x-scheme-handler/whatsapp"),
            Duration::from_secs(5),
        )?;
        if !status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "xdg-mime returned a non-zero exit status",
            ));
        }
        Ok(())
    }

    fn clean_scripts(&self, name: &str) {
        for dir in self.script_dirs() {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();
                if file_name.contains(name) && entry.path().is_file() {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// `NoDisplay=true` and `Hidden=true`, preserving every other line.
pub fn hide_from_launcher(content: &str) -> String {
    let mut out = content.to_string();

    if out.lines().any(|line| line.starts_with("NoDisplay=")) {
        out = out
            .lines()
            .map(|line| {
                if line.starts_with("NoDisplay=") {
                    "NoDisplay=true".to_string()
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    } else {
        while out.ends_with('\n') {
            out.pop();
        }
        out.push_str("\nNoDisplay=true");
    }

    if out.lines().any(|line| line.starts_with("Hidden=")) {
        out = out
            .lines()
            .map(|line| {
                if line.starts_with("Hidden=") {
                    "Hidden=true".to_string()
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    } else {
        out.push_str("\nHidden=true");
    }

    out.push('\n');
    out
}

/// Parse the first token of an `Exec=` line and verify absolute paths.
pub fn is_exec_usable(exec_line: &str) -> bool {
    let Some(rest) = exec_line.strip_prefix("Exec=") else {
        return false;
    };
    let rest = rest.trim();
    let rest = rest.strip_prefix('"').unwrap_or(rest);
    let target: String = rest
        .chars()
        .take_while(|ch| !ch.is_whitespace() && *ch != '"')
        .collect();

    if target.is_empty() {
        return false;
    }
    if !target.starts_with('/') {
        return true; // command from PATH
    }
    Path::new(&target).exists()
}

fn replace_exec_line(entry: &str, exec_line: &str) -> String {
    entry
        .lines()
        .map(|line| {
            if line.starts_with("Exec=") {
                exec_line
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Replace a file atomically so desktop tools never observe a truncated
/// `.desktop` entry (which makes `xdg-mime` report a missing `[Desktop Entry]`
/// group, or worse, hang while the file is being rewritten).
fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "shortcut".to_string());
    let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(".{name}.{}.{suffix}.tmp", std::process::id()));
    fs::write(&temp, contents)?;
    match fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(&temp);
            Err(err)
        }
    }
}

/// Run a helper command without letting a broken desktop environment block
/// the client forever. `xdg-mime` and `update-desktop-database` have been seen
/// to hang on misconfigured systems; a bounded wait keeps the settings window
/// responsive while still doing the integration when the tools work.
fn run_with_timeout(command: &mut Command, timeout: Duration) -> std::io::Result<ExitStatus> {
    let mut child = command.spawn()?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "helper command timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn set_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(permissions.mode() | 0o111);
            let _ = fs::set_permissions(path, permissions);
        }
    }
}

/// `APPIMAGE` points at the stable `.AppImage` file; `current_exe()` inside an
/// AppImage points at a temporary mount that disappears on exit.
pub fn stable_exe_path() -> PathBuf {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_exe().unwrap_or_else(|_| PathBuf::from("whatszan")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_autostart_entries_without_touching_exec() {
        let input = "[Desktop Entry]\nExec=\"/opt/WhatsZan/whatszan\" --hide\nName=WhatsZan\n";
        let output = hide_from_launcher(input);
        assert!(output.contains("NoDisplay=true"));
        assert!(output.contains("Hidden=true"));
        assert!(output.contains("Exec=\"/opt/WhatsZan/whatszan\" --hide"));
    }

    #[test]
    fn exec_parser_accepts_paths_that_exist() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("whatszan");
        fs::write(&exe, b"#!/bin/sh\n").unwrap();
        assert!(is_exec_usable(&format!(
            "Exec=\"{}\" --hide",
            exe.display()
        )));
        assert!(!is_exec_usable("Exec=\"/does/not/exist\" --hide"));
        assert!(is_exec_usable("Exec=whatszan --hide"));
    }

    #[test]
    fn writes_autostart_and_desktop_entries() {
        let home = tempfile::tempdir().unwrap();
        let desktop = home.path().join("Desktop");
        let exe = home.path().join("whatszan");
        fs::write(&exe, b"#!/bin/sh\n").unwrap();
        let integration = DesktopIntegration::new(home.path(), &desktop, &exe);

        integration.apply_auto_run(true, false).unwrap();
        let autostart = home.path().join(".config/autostart/WhatsZan.desktop");
        assert!(autostart.exists());
        let content = fs::read_to_string(&autostart).unwrap();
        assert!(content.contains("--hide"));
        assert!(content.contains("NoDisplay=true"));

        let translations = Map::new();
        let icon = home.path().join("branded-icon.png");
        integration
            .apply_desktop_shortcut(
                true,
                "WhatsZan",
                "WhatsZan",
                &icon.display().to_string(),
                &translations,
            )
            .unwrap();

        let app_entry = home
            .path()
            .join(".local/share/applications/whatszan.desktop");
        assert!(app_entry.exists());
        let app_content = fs::read_to_string(&app_entry).unwrap();
        assert!(app_content.contains("MimeType=x-scheme-handler/whatsapp;"));
        assert!(app_content.contains("StartupWMClass=whatszan"));
        assert!(app_content.contains(&format!("Icon={}", icon.display())));

        // Only one launcher entry: the app-id name used by docks.
        assert!(!home
            .path()
            .join(".local/share/applications/WhatsZan.desktop")
            .exists());

        let shortcut = desktop.join("WhatsZan.desktop");
        assert!(shortcut.exists());
        let shortcut_content = fs::read_to_string(&shortcut).unwrap();
        assert!(shortcut_content.contains(&format!("Icon={}", icon.display())));
    }

    #[test]
    fn renaming_removes_the_old_desktop_shortcut() {
        let home = tempfile::tempdir().unwrap();
        let desktop = home.path().join("Desktop");
        let exe = home.path().join("whatszan");
        fs::write(&exe, b"#!/bin/sh\n").unwrap();
        let integration = DesktopIntegration::new(home.path(), &desktop, &exe);
        let translations = Map::new();
        let icon = home.path().join("icon.png");

        fs::create_dir_all(&desktop).unwrap();
        fs::write(desktop.join("WhatsApp.desktop"), "old").unwrap();
        let applications = home.path().join(".local/share/applications");
        fs::create_dir_all(&applications).unwrap();
        fs::write(applications.join("WhatsZan.desktop"), "old").unwrap();

        integration
            .apply_desktop_shortcut(
                true,
                "WhatsApp",
                "WhatsZan",
                &icon.display().to_string(),
                &translations,
            )
            .unwrap();

        assert!(!desktop.join("WhatsApp.desktop").exists());
        assert!(desktop.join("WhatsZan.desktop").exists());
        assert!(!applications.join("WhatsZan.desktop").exists());
        assert!(applications.join("whatszan.desktop").exists());
    }
}
