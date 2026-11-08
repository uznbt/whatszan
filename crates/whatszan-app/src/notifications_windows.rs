//! Native Windows toast notifications.
//!
//! Electron's renderer used the HTML5 `Notification` API, which Chromium turns
//! into a Windows toast. The Rust build receives the same call from the DOM
//! shim and forwards it to WinRT directly.
//!
//! The toast is registered under the process AUMID. `shell_windows` creates the
//! Start Menu shortcut that Windows requires for an unpackaged desktop app to
//! show notifications under its own identity. Clicking a toast raises the main
//! window and forwards the message id to the renderer, exactly like the Linux
//! backend. The toast also carries an inline reply field; submitting it sends
//! the text back through the renderer's existing `notify-reply` path.
//!
//! The toast XML is built by hand instead of using a builder crate because the
//! reply action needs an `<input>` element and the activation handler must read
//! its `UserInput` map, which toast builder wrappers do not expose.

use std::path::Path;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use whatszan_core::translations::t as translate;
use windows::{
    core::{IInspectable, Interface, Ref, HSTRING},
    Data::Xml::Dom::XmlDocument,
    Foundation::{Collections::ValueSet, IPropertyValue, TypedEventHandler},
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
    UI::Notifications::{ToastActivatedEventArgs, ToastNotification, ToastNotificationManager},
};

use crate::{app_state::AppState, notifications_common, shell_windows::APP_ID, webview};

const REPLY_INPUT_ID: &str = "replyText";
const REPLY_ACTION: &str = "reply";
const CLICK_PREFIX: &str = "click:";
const REPLY_PLACEHOLDER: &str = "Balas pesan...";
const REPLY_BUTTON: &str = "Kirim";

/// Show a toast for a WhatsApp message.
///
/// `icon_data_url` is the sender avatar supplied by the renderer. When it is
/// missing or invalid the current branding icon is used instead, matching the
/// Linux notification backend.
pub fn send(
    app: AppHandle,
    id: String,
    title: String,
    body: String,
    icon_data_url: Option<String>,
) {
    let state = app.state::<AppState>();
    let avatar = icon_data_url
        .as_deref()
        .and_then(|data_url| notifications_common::cache_avatar(&state, data_url));
    let branding = notifications_common::branding_icon(&state);
    let is_avatar = avatar.is_some();
    let image = avatar.or(branding);
    let translations = state.translations();
    let placeholder = translate(
        &translations,
        "notification_reply_placeholder",
        REPLY_PLACEHOLDER,
    );
    let button = translate(&translations, "notification_reply_send", REPLY_BUTTON);
    let xml = toast_xml(
        &id,
        &title,
        &body,
        image.as_deref(),
        is_avatar,
        placeholder,
        button,
    );

    // Tauri commands may run on a thread that has no COM apartment. WinRT
    // needs one; a failed `RoInitialize` usually means the thread is already
    // initialized (possibly as STA by the window code), which is fine too.
    let initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok();

    let click_app = app.clone();
    let click_id = id.clone();
    let handler = TypedEventHandler::<ToastNotification, IInspectable>::new(move |_, args| {
        let reply = activation_reply(args);
        webview::show_main_window(&click_app);

        if let Some(window) = click_app
            .try_state::<AppState>()
            .and_then(|state| state.main_window())
        {
            match reply {
                // The renderer opens the chat, pastes the reply into the
                // compose box, and presses send.
                Some(reply) => {
                    let _ = window.emit("notify-reply", json!([click_id, reply]));
                }
                None => {
                    let _ = window.emit("notify-click", click_id.clone());
                }
            }
        }
        Ok(())
    });

    let result = show_toast(&xml, &handler);
    if let Err(err) = result {
        tracing::warn!(%err, %title, "cannot show Windows toast notification");
    }
    if initialized {
        unsafe { RoUninitialize() };
    }
}

/// Create, register, and display a toast from its XML payload.
fn show_toast(
    xml: &str,
    handler: &TypedEventHandler<ToastNotification, IInspectable>,
) -> windows::core::Result<()> {
    let document = XmlDocument::new()?;
    document.LoadXml(&HSTRING::from(xml))?;

    let toast = ToastNotification::CreateToastNotification(&document)?;
    toast.Activated(handler)?;

    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?;
    notifier.Show(&toast)
}

/// Extract the inline reply text from a toast activation, if any.
fn activation_reply(args: Ref<'_, IInspectable>) -> Option<String> {
    let args = args.as_ref()?;
    let args = args.cast::<ToastActivatedEventArgs>().ok()?;
    args.UserInput().ok().and_then(|input| reply_text(&input))
}

fn reply_text(input: &ValueSet) -> Option<String> {
    let value = input.Lookup(&HSTRING::from(REPLY_INPUT_ID)).ok()?;
    let value = value.cast::<IPropertyValue>().ok()?;
    let text = value.GetString().ok()?.to_string();
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Build the `ToastGeneric` XML.
///
/// `launch` carries the notification id for body clicks. The action button
/// uses `hint-inputId` so Windows returns the typed text in `UserInput`.
fn toast_xml(
    id: &str,
    title: &str,
    body: &str,
    image: Option<&Path>,
    circular_image: bool,
    placeholder: &str,
    button: &str,
) -> String {
    let mut xml = String::from("<toast activationType=\"foreground\" launch=\"");
    xml.push_str(&escape_xml(&format!("{CLICK_PREFIX}{id}")));
    xml.push_str("\"><visual><binding template=\"ToastGeneric\"><text>");
    xml.push_str(&escape_xml(title));
    xml.push_str("</text><text>");
    xml.push_str(&escape_xml(body));
    xml.push_str("</text>");

    if let Some(image) = image {
        let source = escape_xml(&image.display().to_string());
        if circular_image {
            xml.push_str("<image placement=\"appLogoOverride\" hint-crop=\"circle\" src=\"");
        } else {
            xml.push_str("<image placement=\"appLogoOverride\" src=\"");
        }
        xml.push_str(&source);
        xml.push_str("\" alt=\"WhatsZan\"/>");
    }

    xml.push_str("</binding></visual><actions><input id=\"");
    xml.push_str(REPLY_INPUT_ID);
    xml.push_str("\" type=\"text\" placeHolderContent=\"");
    xml.push_str(&escape_xml(placeholder));
    xml.push_str("\"/><action content=\"");
    xml.push_str(&escape_xml(button));
    xml.push_str("\" arguments=\"");
    xml.push_str(REPLY_ACTION);
    xml.push_str("\" hint-inputId=\"");
    xml.push_str(REPLY_INPUT_ID);
    xml.push_str("\" activationType=\"foreground\"/></actions></toast>");

    xml
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_xml_escapes_text_and_adds_reply_action() {
        let xml = toast_xml(
            "id<&",
            "A & B",
            "<hello>",
            Some(Path::new("C:\\tmp\\a&b.png")),
            true,
            "Balas pesan...",
            "Kirim",
        );

        assert!(xml.contains("launch=\"click:id&lt;&amp;\""));
        assert!(xml.contains("<text>A &amp; B</text>"));
        assert!(xml.contains("<text>&lt;hello&gt;</text>"));
        assert!(xml.contains("placement=\"appLogoOverride\""));
        assert!(xml.contains("hint-crop=\"circle\""));
        assert!(xml.contains("src=\"C:\\tmp\\a&amp;b.png\""));
        assert!(xml.contains(
            "<input id=\"replyText\" type=\"text\" placeHolderContent=\"Balas pesan...\"/>"
        ));
        assert!(xml.contains("arguments=\"reply\""));
        assert!(xml.contains("hint-inputId=\"replyText\""));
    }

    #[test]
    fn square_images_omit_the_crop_hint() {
        let xml = toast_xml(
            "1",
            "title",
            "body",
            Some(Path::new("C:\\tmp\\a.png")),
            false,
            "Balas pesan...",
            "Kirim",
        );
        assert!(!xml.contains("hint-crop"));
    }
}
