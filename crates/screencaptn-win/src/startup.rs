use std::io;
use std::path::Path;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::ApplicationModel::{StartupTask, StartupTaskState};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
};

const STARTUP_TASK_ID: &str = "ScreenCapnStartup";
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "ScreenCapn";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunOnStartupState {
    Disabled,
    Enabled,
    DisabledByUser,
    DisabledByPolicy,
    EnabledByPolicy,
}

impl RunOnStartupState {
    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled | Self::EnabledByPolicy)
    }
}

pub fn state() -> RunOnStartupState {
    packaged_task()
        .and_then(|task| task.State())
        .map(packaged_state)
        .unwrap_or_else(|_| {
            if registry_entry_exists() {
                RunOnStartupState::Enabled
            } else {
                RunOnStartupState::Disabled
            }
        })
}

pub fn toggle() -> Result<RunOnStartupState, String> {
    if let Ok(task) = packaged_task() {
        return toggle_packaged(&task);
    }
    let enabled = registry_entry_exists();
    if enabled {
        remove_registry_entry().map_err(|error| error.to_string())?;
        Ok(RunOnStartupState::Disabled)
    } else {
        write_registry_entry().map_err(|error| error.to_string())?;
        Ok(RunOnStartupState::Enabled)
    }
}

fn packaged_task() -> windows::core::Result<StartupTask> {
    StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK_ID))?.get()
}

fn packaged_state(state: StartupTaskState) -> RunOnStartupState {
    if state == StartupTaskState::Enabled {
        RunOnStartupState::Enabled
    } else if state == StartupTaskState::DisabledByUser {
        RunOnStartupState::DisabledByUser
    } else if state == StartupTaskState::DisabledByPolicy {
        RunOnStartupState::DisabledByPolicy
    } else if state == StartupTaskState::EnabledByPolicy {
        RunOnStartupState::EnabledByPolicy
    } else {
        RunOnStartupState::Disabled
    }
}

fn toggle_packaged(task: &StartupTask) -> Result<RunOnStartupState, String> {
    match packaged_state(task.State().map_err(|error| error.to_string())?) {
        RunOnStartupState::Enabled => {
            task.Disable().map_err(|error| error.to_string())?;
            Ok(RunOnStartupState::Disabled)
        }
        RunOnStartupState::Disabled => {
            let state = task
                .RequestEnableAsync()
                .and_then(|operation| operation.get())
                .map(packaged_state)
                .map_err(|error| error.to_string())?;
            match state {
                RunOnStartupState::DisabledByUser => Err(
                    "Windows has disabled Screen Cap'n in Startup Apps. Re-enable it from Windows Settings > Apps > Startup."
                        .to_string(),
                ),
                RunOnStartupState::DisabledByPolicy => Err(
                    "Run on startup is disabled by your organization's Windows policy."
                        .to_string(),
                ),
                _ => Ok(state),
            }
        }
        RunOnStartupState::DisabledByUser => Err(
            "Windows has disabled Screen Cap'n in Startup Apps. Re-enable it from Windows Settings > Apps > Startup."
                .to_string(),
        ),
        RunOnStartupState::DisabledByPolicy | RunOnStartupState::EnabledByPolicy => Err(
            "Run on startup is controlled by your organization's Windows policy.".to_string(),
        ),
    }
}

fn registry_entry_exists() -> bool {
    unsafe {
        let Ok(key) = open_run_key(KEY_QUERY_VALUE) else {
            return false;
        };
        let value_name = wide_null(RUN_VALUE);
        let status = RegQueryValueExW(key.0, PCWSTR(value_name.as_ptr()), None, None, None, None);
        status == ERROR_SUCCESS
    }
}

fn write_registry_entry() -> io::Result<()> {
    let executable = std::env::current_exe()?;
    let command = quoted_startup_command(&executable);
    let bytes = unsafe {
        std::slice::from_raw_parts(
            command.as_ptr().cast::<u8>(),
            command.len() * size_of::<u16>(),
        )
    };
    unsafe {
        let key = create_run_key()?;
        let value_name = wide_null(RUN_VALUE);
        win32_result(RegSetValueExW(
            key.0,
            PCWSTR(value_name.as_ptr()),
            0,
            REG_SZ,
            Some(bytes),
        ))
    }
}

fn remove_registry_entry() -> io::Result<()> {
    unsafe {
        let Ok(key) = open_run_key(KEY_SET_VALUE) else {
            return Ok(());
        };
        let value_name = wide_null(RUN_VALUE);
        let status = RegDeleteValueW(key.0, PCWSTR(value_name.as_ptr()));
        if status == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            win32_result(status)
        }
    }
}

struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

unsafe fn open_run_key(
    access: windows::Win32::System::Registry::REG_SAM_FLAGS,
) -> io::Result<RegistryKey> {
    let mut key = HKEY::default();
    let path = wide_null(RUN_KEY);
    win32_result(RegOpenKeyExW(
        HKEY_CURRENT_USER,
        PCWSTR(path.as_ptr()),
        0,
        access,
        &mut key,
    ))?;
    Ok(RegistryKey(key))
}

unsafe fn create_run_key() -> io::Result<RegistryKey> {
    let mut key = HKEY::default();
    let path = wide_null(RUN_KEY);
    win32_result(RegCreateKeyExW(
        HKEY_CURRENT_USER,
        PCWSTR(path.as_ptr()),
        0,
        PWSTR::null(),
        REG_OPTION_NON_VOLATILE,
        KEY_SET_VALUE,
        None,
        &mut key,
        None,
    ))?;
    Ok(RegistryKey(key))
}

fn win32_result(status: windows::Win32::Foundation::WIN32_ERROR) -> io::Result<()> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status.0 as i32))
    }
}

fn quoted_startup_command(executable: &Path) -> Vec<u16> {
    format!("\"{}\" --startup", executable.display())
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn startup_command_quotes_paths_with_spaces() {
        let command = quoted_startup_command(&PathBuf::from(
            r"C:\Program Files\Screen Cap'n\screencaptn.exe",
        ));
        let command = String::from_utf16(&command[..command.len() - 1]).unwrap();
        assert_eq!(
            command,
            r#""C:\Program Files\Screen Cap'n\screencaptn.exe" --startup"#
        );
    }
}
