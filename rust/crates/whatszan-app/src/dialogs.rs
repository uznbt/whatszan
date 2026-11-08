//! Native message dialogs.
//!
//! `rfd`'s xdg-portal backend falls back to `zenity` for message dialogs, which
//! is not installed on every desktop. GTK is already a hard dependency of the
//! Tauri build, so use it directly on Linux. Other platforms keep the `rfd`
//! implementation.

#[derive(Debug, Clone, Copy)]
pub enum Level {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Buttons {
    pub yes: String,
    pub no: Option<String>,
}

impl Buttons {
    pub fn ok(label: impl Into<String>) -> Self {
        Self {
            yes: label.into(),
            no: None,
        }
    }

    pub fn yes_no(yes: impl Into<String>, no: impl Into<String>) -> Self {
        Self {
            yes: yes.into(),
            no: Some(no.into()),
        }
    }
}

/// Show a message dialog and wait for the user's choice.
///
/// Returns `true` for the affirmative button (`OK` or `Yes`) and `false` for
/// `No` or when the dialog is closed without a response.
pub async fn message(
    level: Level,
    title: impl Into<String>,
    text: impl Into<String>,
    buttons: Buttons,
) -> bool {
    let title = title.into();
    let text = text.into();

    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;

        let (tx, rx) = tokio::sync::oneshot::channel();
        gtk::glib::MainContext::default().invoke(move || {
            let dialog = gtk::MessageDialog::builder()
                .message_type(match level {
                    Level::Info => gtk::MessageType::Info,
                    Level::Warning => gtk::MessageType::Warning,
                    Level::Error => gtk::MessageType::Error,
                })
                .buttons(gtk::ButtonsType::None)
                .text(text)
                .use_markup(false)
                .build();
            dialog.set_title(&title);
            dialog.set_modal(true);
            dialog.add_button(&buttons.yes, gtk::ResponseType::Yes);
            if let Some(no) = buttons.no.as_deref() {
                dialog.add_button(no, gtk::ResponseType::No);
            }
            dialog.set_default_response(gtk::ResponseType::Yes);

            let sender = std::cell::RefCell::new(Some(tx));
            dialog.connect_response(move |dialog, response| {
                dialog.close();
                if let Some(tx) = sender.borrow_mut().take() {
                    let _ = tx.send(matches!(
                        response,
                        gtk::ResponseType::Yes | gtk::ResponseType::Ok
                    ));
                }
            });
            dialog.show_all();
        });
        rx.await.unwrap_or(false)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let result = rfd::AsyncMessageDialog::new()
            .set_level(match level {
                Level::Info => rfd::MessageLevel::Info,
                Level::Warning => rfd::MessageLevel::Warning,
                Level::Error => rfd::MessageLevel::Error,
            })
            .set_title(&title)
            .set_description(&text)
            .set_buttons(if buttons.no.is_some() {
                rfd::MessageButtons::YesNo
            } else {
                rfd::MessageButtons::Ok
            })
            .show()
            .await;
        matches!(
            result,
            rfd::MessageDialogResult::Yes | rfd::MessageDialogResult::Ok
        )
    }
}
