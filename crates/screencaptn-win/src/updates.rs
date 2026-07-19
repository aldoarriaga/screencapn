use crate::settings::{PendingUpdate, ReleaseNotes, UpdateCheckSettings};
use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use windows::core::{w, Interface, PCWSTR};
use windows::ApplicationModel::Package;
use windows::Foundation::Collections::{IIterable, IVectorView};
use windows::Foundation::{AsyncOperationCompletedHandler, AsyncStatus};
use windows::Services::Store::{
    StoreContext, StorePackageUpdate, StorePackageUpdateResult, StorePackageUpdateState,
};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetOption, WinHttpSetTimeouts, INTERNET_DEFAULT_HTTPS_PORT,
    WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_OPTION_REDIRECT_POLICY,
    WINHTTP_OPTION_REDIRECT_POLICY_NEVER, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};
use windows::Win32::UI::Shell::IInitializeWithWindow;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_UPDATE_EVENT: u32 = WM_APP + 0x72;
pub const WM_UPDATE_INSTALL_READY: u32 = WM_APP + 0x73;
const WEEK_SECONDS: i64 = 7 * 24 * 60 * 60;
const RETRY_SECONDS: i64 = 24 * 60 * 60;
const NOTES_HOST: &str = "screencapn.com";
const NOTES_LIMIT_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub struct UpdateService {
    runtime: Arc<Mutex<UpdateRuntime>>,
}

#[derive(Default)]
struct UpdateRuntime {
    checking: bool,
    installing: bool,
    events: Vec<UpdateEvent>,
}

struct InstallPayload {
    context: StoreContext,
    updates: IVectorView<StorePackageUpdate>,
}

#[derive(Clone, Debug)]
pub enum UpdateEvent {
    CheckCompleted(UpdateCheckOutcome),
    InstallCompleted(UpdateInstallOutcome),
}

#[derive(Clone, Debug)]
pub enum UpdateCheckOutcome {
    NoUpdate,
    Available(PendingUpdate),
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateInstallOutcome {
    Completed,
    NoUpdate,
    Failed,
}

impl Default for UpdateService {
    fn default() -> Self {
        Self {
            runtime: Arc::new(Mutex::new(UpdateRuntime::default())),
        }
    }
}

impl UpdateService {
    pub fn begin_due_check(&self, hwnd: HWND, settings: &UpdateCheckSettings) {
        if !is_store_packaged() || !check_is_due(settings, now_unix_seconds()) {
            return;
        }
        let Ok(mut runtime) = self.runtime.lock() else {
            return;
        };
        if runtime.checking || runtime.installing {
            return;
        }
        let context = match store_context_for_window(hwnd) {
            Ok(context) => context,
            Err(_) => {
                runtime
                    .events
                    .push(UpdateEvent::CheckCompleted(UpdateCheckOutcome::Failed));
                drop(runtime);
                post_update_event(hwnd.0 as usize);
                return;
            }
        };
        let operation = match context.GetAppAndOptionalStorePackageUpdatesAsync() {
            Ok(operation) => operation,
            Err(_) => {
                runtime
                    .events
                    .push(UpdateEvent::CheckCompleted(UpdateCheckOutcome::Failed));
                drop(runtime);
                post_update_event(hwnd.0 as usize);
                return;
            }
        };
        runtime.checking = true;
        drop(runtime);

        let runtime = Arc::clone(&self.runtime);
        let hwnd_value = hwnd.0 as usize;
        let _ = operation.SetCompleted(&AsyncOperationCompletedHandler::new(
            move |operation, status| {
                let updates = operation
                    .filter(|_| status == AsyncStatus::Completed)
                    .and_then(|operation| operation.GetResults().ok());
                if let Some(updates) = updates {
                    if let Some(mut pending) = pending_update_from_store(&updates) {
                        let runtime = Arc::clone(&runtime);
                        std::thread::spawn(move || {
                            pending.release_notes = fetch_release_notes(&pending.version);
                            push_event(
                                &runtime,
                                UpdateEvent::CheckCompleted(UpdateCheckOutcome::Available(pending)),
                            );
                            post_update_event(hwnd_value);
                        });
                    } else {
                        push_event(
                            &runtime,
                            UpdateEvent::CheckCompleted(UpdateCheckOutcome::NoUpdate),
                        );
                        post_update_event(hwnd_value);
                    }
                } else {
                    push_event(
                        &runtime,
                        UpdateEvent::CheckCompleted(UpdateCheckOutcome::Failed),
                    );
                    post_update_event(hwnd_value);
                }
                if let Ok(mut guard) = runtime.lock() {
                    guard.checking = false;
                }
                Ok(())
            },
        ));
    }

    pub fn begin_install(&self, hwnd: HWND) {
        let Ok(mut runtime) = self.runtime.lock() else {
            return;
        };
        if runtime.installing {
            return;
        }
        let Ok(context) = store_context_for_window(hwnd) else {
            runtime
                .events
                .push(UpdateEvent::InstallCompleted(UpdateInstallOutcome::Failed));
            drop(runtime);
            post_update_event(hwnd.0 as usize);
            return;
        };
        let Ok(operation) = context.GetAppAndOptionalStorePackageUpdatesAsync() else {
            runtime
                .events
                .push(UpdateEvent::InstallCompleted(UpdateInstallOutcome::Failed));
            drop(runtime);
            post_update_event(hwnd.0 as usize);
            return;
        };
        runtime.installing = true;
        drop(runtime);

        let runtime = Arc::clone(&self.runtime);
        let hwnd_value = hwnd.0 as usize;
        let _ = operation.SetCompleted(&AsyncOperationCompletedHandler::new(
            move |operation, status| {
                let updates = operation
                    .filter(|_| status == AsyncStatus::Completed)
                    .and_then(|operation| operation.GetResults().ok());
                let Some(updates) = updates.filter(|updates: &IVectorView<StorePackageUpdate>| {
                    updates.Size().unwrap_or_default() > 0
                }) else {
                    finish_install(&runtime, UpdateInstallOutcome::NoUpdate, hwnd_value);
                    return Ok(());
                };
                let payload = Box::new(InstallPayload {
                    context: context.clone(),
                    updates,
                });
                let payload_ptr = Box::into_raw(payload);
                let posted = unsafe {
                    PostMessageW(
                        HWND(hwnd_value as *mut c_void),
                        WM_UPDATE_INSTALL_READY,
                        WPARAM(0),
                        LPARAM(payload_ptr as isize),
                    )
                    .is_ok()
                };
                if !posted {
                    unsafe {
                        let _ = Box::from_raw(payload_ptr);
                    }
                    finish_install(&runtime, UpdateInstallOutcome::Failed, hwnd_value);
                }
                Ok(())
            },
        ));
    }

    pub unsafe fn start_install_from_message(&self, hwnd: HWND, payload: LPARAM) {
        let payload = payload.0 as *mut InstallPayload;
        if payload.is_null() {
            finish_install(&self.runtime, UpdateInstallOutcome::Failed, hwnd.0 as usize);
            return;
        }
        let payload = Box::from_raw(payload);
        let Ok(iterable) = payload.updates.cast::<IIterable<StorePackageUpdate>>() else {
            finish_install(&self.runtime, UpdateInstallOutcome::Failed, hwnd.0 as usize);
            return;
        };
        let Ok(operation) = payload
            .context
            .RequestDownloadAndInstallStorePackageUpdatesAsync(&iterable)
        else {
            finish_install(&self.runtime, UpdateInstallOutcome::Failed, hwnd.0 as usize);
            return;
        };
        let runtime = Arc::clone(&self.runtime);
        let hwnd_value = hwnd.0 as usize;
        let _ = operation.SetCompleted(
            &windows::Foundation::AsyncOperationWithProgressCompletedHandler::new(
                move |operation, status| {
                    let outcome = operation
                        .filter(|_| status == AsyncStatus::Completed)
                        .and_then(|operation| operation.GetResults().ok())
                        .and_then(|result: StorePackageUpdateResult| result.OverallState().ok())
                        .map(|state| {
                            if state == StorePackageUpdateState::Completed {
                                UpdateInstallOutcome::Completed
                            } else {
                                UpdateInstallOutcome::Failed
                            }
                        })
                        .unwrap_or(UpdateInstallOutcome::Failed);
                    finish_install(&runtime, outcome, hwnd_value);
                    Ok(())
                },
            ),
        );
    }

    pub fn take_events(&self) -> Vec<UpdateEvent> {
        let Ok(mut runtime) = self.runtime.lock() else {
            return Vec::new();
        };
        std::mem::take(&mut runtime.events)
    }
}

pub fn apply_check_outcome(settings: &mut UpdateCheckSettings, outcome: UpdateCheckOutcome) {
    let now = now_unix_seconds();
    match outcome {
        UpdateCheckOutcome::NoUpdate => {
            settings.pending = None;
            settings.last_successful_check_unix_seconds = Some(now);
            settings.retry_after_unix_seconds = None;
        }
        UpdateCheckOutcome::Available(pending) => {
            settings.pending = Some(pending);
            settings.last_successful_check_unix_seconds = Some(now);
            settings.retry_after_unix_seconds = None;
        }
        UpdateCheckOutcome::Failed => {
            settings.retry_after_unix_seconds = Some(now + RETRY_SECONDS);
        }
    }
}

pub fn clear_installed_pending_update(settings: &mut UpdateCheckSettings) -> bool {
    let Some(pending) = &settings.pending else {
        return false;
    };
    let Some(installed) = installed_package_version() else {
        return false;
    };
    if compare_versions(&installed, &pending.version).is_ge() {
        settings.pending = None;
        return true;
    }
    false
}

pub fn details_url(version: &str) -> String {
    format!("https://{NOTES_HOST}/updates/{version}")
}

fn finish_install(runtime: &Arc<Mutex<UpdateRuntime>>, outcome: UpdateInstallOutcome, hwnd: usize) {
    if let Ok(mut guard) = runtime.lock() {
        guard.installing = false;
        guard.events.push(UpdateEvent::InstallCompleted(outcome));
    }
    post_update_event(hwnd);
}

fn push_event(runtime: &Arc<Mutex<UpdateRuntime>>, event: UpdateEvent) {
    if let Ok(mut guard) = runtime.lock() {
        guard.events.push(event);
    }
}

fn post_update_event(hwnd: usize) {
    unsafe {
        let _ = PostMessageW(
            HWND(hwnd as *mut c_void),
            WM_UPDATE_EVENT,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

fn store_context_for_window(hwnd: HWND) -> windows::core::Result<StoreContext> {
    let context = StoreContext::GetDefault()?;
    let owner: IInitializeWithWindow = context.cast()?;
    unsafe {
        owner.Initialize(hwnd)?;
    }
    Ok(context)
}

fn pending_update_from_store(updates: &IVectorView<StorePackageUpdate>) -> Option<PendingUpdate> {
    let update = updates.GetAt(0).ok()?;
    let package = update.Package().ok()?;
    let version = package.Id().ok()?.Version().ok()?;
    Some(PendingUpdate {
        version: format_package_version(
            version.Major,
            version.Minor,
            version.Build,
            version.Revision,
        ),
        release_notes: None,
    })
}

fn is_store_packaged() -> bool {
    Package::Current().is_ok()
}

fn installed_package_version() -> Option<String> {
    let version = Package::Current().ok()?.Id().ok()?.Version().ok()?;
    Some(format_package_version(
        version.Major,
        version.Minor,
        version.Build,
        version.Revision,
    ))
}

fn check_is_due(settings: &UpdateCheckSettings, now: i64) -> bool {
    if settings
        .retry_after_unix_seconds
        .is_some_and(|retry| retry > now)
    {
        return false;
    }
    settings
        .last_successful_check_unix_seconds
        .is_none_or(|last| now.saturating_sub(last) >= WEEK_SECONDS)
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn format_package_version(major: u16, minor: u16, build: u16, revision: u16) -> String {
    format!("{major}.{minor}.{build}.{revision}")
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let parse = |value: &str| {
        let mut parts = value
            .split('.')
            .map(|part| part.parse::<u16>().unwrap_or_default());
        [
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default(),
        ]
    };
    parse(left).cmp(&parse(right))
}

fn fetch_release_notes(version: &str) -> Option<ReleaseNotes> {
    let bytes = fetch_https(&format!("/updates/{version}.json"))?;
    let notes: ReleaseNotes = serde_json::from_slice(&bytes).ok()?;
    normalize_release_notes(notes, version)
}

fn normalize_release_notes(mut notes: ReleaseNotes, version: &str) -> Option<ReleaseNotes> {
    if notes.version != version || notes.title.trim().is_empty() {
        return None;
    }
    notes.title = notes.title.trim().chars().take(120).collect();
    notes.highlights = notes
        .highlights
        .into_iter()
        .filter_map(|highlight| {
            let trimmed = highlight.trim();
            (!trimmed.is_empty()).then(|| trimmed.chars().take(180).collect::<String>())
        })
        .take(3)
        .collect();
    Some(notes)
}

fn fetch_https(path: &str) -> Option<Vec<u8>> {
    unsafe {
        let session = WinHttpHandle::new(WinHttpOpen(
            w!("ScreenCapn Update Check"),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, 3_000, 5_000, 5_000, 5_000).ok()?;
        let server = wide_null(NOTES_HOST);
        let connection = WinHttpHandle::new(WinHttpConnect(
            session.0,
            PCWSTR(server.as_ptr()),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        ))?;
        let object_name = wide_null(path);
        let request = WinHttpHandle::new(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            PCWSTR(object_name.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        let redirect_policy = WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_ne_bytes();
        WinHttpSetOption(
            Some(request.0.cast()),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&redirect_policy),
        )
        .ok()?;
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).ok()?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).ok()?;

        let mut status = 0u32;
        let mut status_length = std::mem::size_of::<u32>() as u32;
        let mut header_index = 0u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut status_length,
            &mut header_index,
        )
        .ok()?;
        if status != 200 {
            return None;
        }

        let mut bytes = Vec::new();
        loop {
            let mut available = 0u32;
            WinHttpQueryDataAvailable(request.0, &mut available).ok()?;
            if available == 0 {
                break;
            }
            let remaining = NOTES_LIMIT_BYTES.checked_sub(bytes.len())?;
            if available as usize > remaining {
                return None;
            }
            let start = bytes.len();
            bytes.resize(start + available as usize, 0);
            let mut read = 0u32;
            WinHttpReadData(
                request.0,
                bytes[start..].as_mut_ptr().cast(),
                available,
                &mut read,
            )
            .ok()?;
            bytes.truncate(start + read as usize);
        }
        Some(bytes)
    }
}

struct WinHttpHandle(*mut c_void);

impl WinHttpHandle {
    fn new(handle: *mut c_void) -> Option<Self> {
        (!handle.is_null()).then_some(Self(handle))
    }
}

impl Drop for WinHttpHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_check_waits_a_week_after_success() {
        let settings = UpdateCheckSettings {
            last_successful_check_unix_seconds: Some(100),
            ..Default::default()
        };
        assert!(!check_is_due(&settings, 100 + WEEK_SECONDS - 1));
        assert!(check_is_due(&settings, 100 + WEEK_SECONDS));
    }

    #[test]
    fn retry_delay_overrides_regular_schedule() {
        let settings = UpdateCheckSettings {
            retry_after_unix_seconds: Some(200),
            ..Default::default()
        };
        assert!(!check_is_due(&settings, 199));
        assert!(check_is_due(&settings, 200));
    }

    #[test]
    fn package_versions_compare_by_all_four_segments() {
        assert!(compare_versions("1.0.2.0", "1.0.1.0").is_gt());
        assert!(compare_versions("1.0.1.0", "1.0.1.0").is_eq());
        assert!(compare_versions("1.0.0.0", "1.0.1.0").is_lt());
    }

    #[test]
    fn notes_require_the_store_version_and_keep_only_three_highlights() {
        let notes = ReleaseNotes {
            version: "1.0.2.0".to_string(),
            title: "  Ready for launch  ".to_string(),
            highlights: vec![
                " First ".to_string(),
                "Second".to_string(),
                "Third".to_string(),
                "Fourth".to_string(),
            ],
        };
        let normalized = normalize_release_notes(notes, "1.0.2.0").unwrap();
        assert_eq!(normalized.title, "Ready for launch");
        assert_eq!(normalized.highlights, ["First", "Second", "Third"]);
        assert!(normalize_release_notes(
            ReleaseNotes {
                version: "1.0.1.0".to_string(),
                title: "Wrong version".to_string(),
                highlights: vec![],
            },
            "1.0.2.0",
        )
        .is_none());
    }
}
