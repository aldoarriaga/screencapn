use crate::diagnostics;

use std::ffi::c_void;
use std::ptr;
use std::sync::mpsc;

use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::core::{Interface, PWSTR};
use windows::Win32::Foundation::{E_POINTER, HWND, RECT};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::WinRT::EventRegistrationToken;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

type HostMessageCallback = unsafe fn(*mut c_void, String);

pub struct WebUi {
    controller: ICoreWebView2Controller,
    webview: ICoreWebView2,
    _message_token: EventRegistrationToken,
    _process_failed_token: EventRegistrationToken,
}

impl WebUi {
    pub unsafe fn create(
        parent: HWND,
        context: *mut c_void,
        callback: HostMessageCallback,
    ) -> webview2_com::Result<Self> {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(webview2_com::Error::WindowsError)?;

        let environment = create_environment()?;
        let controller = create_controller(&environment, parent)?;
        set_controller_bounds(&controller, parent)?;
        controller.SetIsVisible(false)?;

        if let Ok(controller2) = controller.cast::<ICoreWebView2Controller2>() {
            controller2.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                A: 0,
                R: 0,
                G: 0,
                B: 0,
            })?;
        }

        let webview = controller.CoreWebView2()?;
        let context = context as isize;
        if let Ok(settings) = webview.Settings() {
            let _ = settings.SetAreDefaultContextMenusEnabled(false);
            let _ = settings.SetAreDevToolsEnabled(false);
            let _ = settings.SetIsStatusBarEnabled(false);
        }

        let mut process_failed_token = EventRegistrationToken::default();
        let process_failed_context = context;
        webview.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_sender, args| {
                let details = format_webview_process_failed(args);
                diagnostics::log_event("webview", &details);
                let message = serde_json::json!({
                    "type": "webviewProcessFailed",
                    "reason": details,
                })
                .to_string();
                unsafe {
                    callback(process_failed_context as *mut c_void, message);
                }
                Ok(())
            })),
            &mut process_failed_token,
        )?;

        let mut message_token = EventRegistrationToken::default();
        webview.add_WebMessageReceived(
            &WebMessageReceivedEventHandler::create(Box::new(move |_sender, args| {
                if let Some(args) = args {
                    let mut message = PWSTR(ptr::null_mut());
                    if args.WebMessageAsJson(&mut message).is_ok() {
                        let message = CoTaskMemPWSTR::from(message);
                        callback(context as *mut c_void, message.to_string());
                    }
                }
                Ok(())
            })),
            &mut message_token,
        )?;

        let html = web_ui_html();
        let html = CoTaskMemPWSTR::from(html.as_str());
        webview.NavigateToString(*html.as_ref().as_pcwstr())?;

        Ok(Self {
            controller,
            webview,
            _message_token: message_token,
            _process_failed_token: process_failed_token,
        })
    }

    pub fn post_json(&self, json: &str) {
        unsafe {
            let json = CoTaskMemPWSTR::from(json);
            let _ = self
                .webview
                .PostWebMessageAsJson(*json.as_ref().as_pcwstr());
        }
    }

    pub fn set_visible(&self, visible: bool) {
        unsafe {
            let _ = self.controller.SetIsVisible(visible);
        }
    }
}

impl Drop for WebUi {
    fn drop(&mut self) {
        unsafe {
            let _ = self.controller.Close();
        }
    }
}

fn format_webview_process_failed(args: Option<ICoreWebView2ProcessFailedEventArgs>) -> String {
    let Some(args) = args else {
        return "process-failed args=<none>".to_string();
    };
    unsafe {
        let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND(0);
        let _ = args.ProcessFailedKind(&mut kind);
        let mut message = format!("process-failed kind={}", kind.0);
        if let Ok(args2) = args.cast::<ICoreWebView2ProcessFailedEventArgs2>() {
            let mut reason = COREWEBVIEW2_PROCESS_FAILED_REASON(0);
            let mut exit_code = 0;
            let _ = args2.Reason(&mut reason);
            let _ = args2.ExitCode(&mut exit_code);
            message.push_str(&format!(" reason={} exit_code={}", reason.0, exit_code));
        }
        message
    }
}
unsafe fn create_environment() -> webview2_com::Result<ICoreWebView2Environment> {
    let (tx, rx) = mpsc::channel();
    CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
        Box::new(|handler| {
            CreateCoreWebView2Environment(&handler).map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |error_code, environment| {
            error_code?;
            tx.send(environment.ok_or_else(|| windows::core::Error::from(E_POINTER)))
                .expect("send WebView2 environment");
            Ok(())
        }),
    )?;

    rx.recv()
        .map_err(|_| webview2_com::Error::SendError)?
        .map_err(webview2_com::Error::WindowsError)
}

unsafe fn create_controller(
    environment: &ICoreWebView2Environment,
    parent: HWND,
) -> webview2_com::Result<ICoreWebView2Controller> {
    let (tx, rx) = mpsc::channel();
    let environment = environment.clone();
    CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| {
            environment
                .CreateCoreWebView2Controller(parent, &handler)
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |error_code, controller| {
            error_code?;
            tx.send(controller.ok_or_else(|| windows::core::Error::from(E_POINTER)))
                .expect("send WebView2 controller");
            Ok(())
        }),
    )?;

    rx.recv()
        .map_err(|_| webview2_com::Error::SendError)?
        .map_err(webview2_com::Error::WindowsError)
}

unsafe fn set_controller_bounds(
    controller: &ICoreWebView2Controller,
    parent: HWND,
) -> windows::core::Result<()> {
    let mut rect = RECT::default();
    GetClientRect(parent, &mut rect)?;
    controller.SetBounds(rect)
}

fn web_ui_html() -> String {
    let html = include_str!("../assets/web-ui/index.html");
    html.replace(
        "/*__KONVA__*/",
        &script_safe(include_str!("../assets/web-ui/vendor/konva.js")),
    )
    .replace(
        "/*__APP__*/",
        &script_safe(include_str!("../assets/web-ui/app.bundle.js")),
    )
    .replace("__ICONS_JSON__", &icons_json())
}

fn script_safe(script: &str) -> String {
    script.replace("</script", "<\\/script")
}

fn icons_json() -> String {
    serde_json::json!({
        "gripLight": include_str!("../assets/toolbar/drag-bkcg-light.svg"),
        "gripDark": include_str!("../assets/toolbar/drag-bkcg-dark.svg"),
        "numbering": include_str!("../assets/toolbar/numbering.svg"),
        "rectangle": include_str!("../assets/toolbar/rectangle.svg"),
        "oval": include_str!("../assets/toolbar/ellipse.svg"),
        "line": include_str!("../assets/toolbar/line.svg"),
        "arrow": include_str!("../assets/toolbar/arrow.svg"),
        "pen": include_str!("../assets/toolbar/pen.svg"),
        "highlighter": include_str!("../assets/toolbar/highlighter.svg"),
        "text": include_str!("../assets/toolbar/text.svg"),
        "tag": include_str!("../assets/toolbar/tag.svg"),
        "watermark": include_str!("../assets/toolbar/watermark.svg"),
        "mosaic": include_str!("../assets/toolbar/pixelate.svg"),
        "undo": include_str!("../assets/toolbar/undo.svg"),
        "copy": include_str!("../assets/toolbar/copy.svg"),
        "save": include_str!("../assets/toolbar/save.svg"),
        "cancel": include_str!("../assets/toolbar/cancel.svg"),
        "lightMode": include_str!("../assets/toolbar/light-mode.svg"),
        "darkMode": include_str!("../assets/toolbar/dark-mode.svg"),
        "miniLine": include_str!("../assets/toolbar/mini-line.svg"),
        "miniArrow": include_str!("../assets/toolbar/mini-arrow.svg"),
        "area": include_str!("../assets/toolbar/area.svg"),
        "lineText": include_str!("../assets/toolbar/line-text.svg"),
        "solidText": include_str!("../assets/toolbar/solid-text.svg"),
        "calendar": include_str!("../assets/toolbar/calendar.svg"),
        "image": include_str!("../assets/toolbar/image.svg"),
        "smallerFont": include_str!("../assets/toolbar/smaller-font.svg"),
        "largerFont": include_str!("../assets/toolbar/larger-font.svg"),
        "restartNumbering": include_str!("../assets/toolbar/restart-numbering.svg"),
        "continueNumbering": include_str!("../assets/toolbar/continue-numbering.svg"),
        "regionLocked": include_str!("../assets/region-controls/locked.svg"),
        "regionUnlocked": include_str!("../assets/region-controls/unlocked.svg"),
        "ratioCustom": include_str!("../assets/region-controls/ratio-custom.svg"),
        "ratio9x16": include_str!("../assets/region-controls/ratio-9x16.svg"),
        "ratio16x9": include_str!("../assets/region-controls/ratio-16x9.svg"),
        "ratio1x1": include_str!("../assets/region-controls/ratio-1x1.svg"),
        "ratio4x5": include_str!("../assets/region-controls/ratio-4x5.svg")
    })
    .to_string()
}
