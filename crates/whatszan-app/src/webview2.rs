//! WebView2 integration for the Windows build.
//!
//! The Linux build configures WebKitGTK directly (media streams, permission
//! requests, service-worker notifications). WebView2 exposes the same hooks
//! through COM interfaces; this module installs the equivalent handlers on a
//! window's `ICoreWebView2` instance.

use tauri::{Manager, WebviewWindow};
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{
        ICoreWebView2, ICoreWebView2Settings2, ICoreWebView2_8, COREWEBVIEW2_PERMISSION_STATE_ALLOW,
    },
    PermissionRequestedEventHandler,
};
use windows::core::{Interface, HSTRING};

use crate::app_state::AppState;

/// Install permission, user-agent, and mute handlers on a window.
///
/// `main` is kept for symmetry with the Linux implementation: popups receive
/// the same permission handler but not the main window's load-failure retry.
pub fn install(window: &WebviewWindow, _main: bool) {
    let app = window.app_handle().clone();
    let _ = window.with_webview(move |webview| {
        let controller = webview.controller();
        let Ok(core) = (unsafe { controller.CoreWebView2() }) else {
            return;
        };

        allow_permissions(&core);

        if let Some(state) = app.try_state::<AppState>() {
            set_user_agent_core(&core, &state.user_agent());
            set_muted_core(&core, state.is_dnd());
        }
    });
}

/// Update the WebView2 user agent after the settings page changes it.
pub fn set_user_agent(window: &WebviewWindow, user_agent: &str) {
    let user_agent = user_agent.to_string();
    let _ = window.with_webview(move |webview| {
        let controller = webview.controller();
        if let Ok(core) = unsafe { controller.CoreWebView2() } {
            set_user_agent_core(&core, &user_agent);
        }
    });
}

/// Mute or unmute the WebView2 audio output for Do Not Disturb.
pub fn set_muted(window: &WebviewWindow, muted: bool) {
    let _ = window.with_webview(move |webview| {
        let controller = webview.controller();
        if let Ok(core) = unsafe { controller.CoreWebView2() } {
            set_muted_core(&core, muted);
        }
    });
}

fn allow_permissions(core: &ICoreWebView2) {
    let handler = PermissionRequestedEventHandler::create(Box::new(|_sender, args| {
        if let Some(args) = args {
            let _ = unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW) };
        }
        Ok(())
    }));

    let mut token = 0i64;
    if let Err(err) = unsafe { core.add_PermissionRequested(&handler, &mut token) } {
        tracing::warn!(%err, "cannot install WebView2 permission handler");
    }
}

fn set_user_agent_core(core: &ICoreWebView2, user_agent: &str) {
    let Ok(settings) = (unsafe { core.Settings() }) else {
        return;
    };
    let Ok(settings) = settings.cast::<ICoreWebView2Settings2>() else {
        tracing::debug!("WebView2 runtime does not expose ICoreWebView2Settings2");
        return;
    };
    if let Err(err) = unsafe { settings.SetUserAgent(&HSTRING::from(user_agent)) } {
        tracing::warn!(%err, "cannot update WebView2 user agent");
    }
}

fn set_muted_core(core: &ICoreWebView2, muted: bool) {
    let Ok(core8) = core.cast::<ICoreWebView2_8>() else {
        tracing::debug!("WebView2 runtime does not expose ICoreWebView2_8");
        return;
    };
    if let Err(err) = unsafe { core8.SetIsMuted(muted) } {
        tracing::warn!(%err, "cannot update WebView2 mute state");
    }
}
