use std::ffi::c_void;
use std::io;
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::ptr::{null, null_mut};
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Power::{
    DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, ES_DISPLAY_REQUIRED, GetSystemPowerStatus, HPOWERNOTIFY,
    PowerClearRequest, PowerCreateRequest, PowerRequestDisplayRequired, PowerRequestSystemRequired,
    PowerSetRequest, PowerSettingRegisterNotification, PowerSettingUnregisterNotification,
    SYSTEM_POWER_STATUS, SetThreadExecutionState,
};
use windows_sys::Win32::System::SystemServices::{
    GUID_ACDC_POWER_SOURCE, POWER_REQUEST_CONTEXT_VERSION,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0, SetEvent,
};
use windows_sys::Win32::UI::WindowsAndMessaging::DEVICE_NOTIFY_CALLBACK;

// A callback can still signal the event during unregistration.
static POWER_CHANGED: OnceLock<OwnedHandle> = OnceLock::new();

pub struct PowerSource {
    registration: HPOWERNOTIFY,
    event: &'static OwnedHandle,
}

impl PowerSource {
    pub fn subscribe() -> Result<Self, String> {
        let event = if let Some(event) = POWER_CHANGED.get() {
            event
        } else {
            let handle = unsafe { CreateEventW(null(), 0, 0, null()) };
            check(!handle.is_null(), "Creating the power notification event")?;
            // SAFETY: CreateEventW returned an owned handle.
            let event = unsafe { OwnedHandle::from_raw_handle(handle) };
            POWER_CHANGED.get_or_init(|| event)
        };

        let parameters: &'static _ = &DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(power_changed),
            Context: null_mut(),
        };
        let mut registration = null_mut();
        // SAFETY: The callback and its parameters outlive Win32's registration.
        let status = unsafe {
            PowerSettingRegisterNotification(
                &GUID_ACDC_POWER_SOURCE,
                DEVICE_NOTIFY_CALLBACK,
                std::ptr::from_ref(parameters).cast_mut().cast(),
                &mut registration,
            )
        };
        if status != ERROR_SUCCESS {
            let error = io::Error::from_raw_os_error(status as i32);
            return Err(format!(
                "Registering for power-source changes failed. {error}"
            ));
        }
        Ok(Self {
            registration: registration as HPOWERNOTIFY,
            event,
        })
    }

    pub fn event(&self) -> BorrowedHandle<'_> {
        self.event.as_handle()
    }

    pub fn on_ac(&self) -> Result<bool, String> {
        let mut status = SYSTEM_POWER_STATUS::default();
        check(
            unsafe { GetSystemPowerStatus(&mut status) } != 0,
            "Reading the power source",
        )?;
        Ok(status.ACLineStatus == 1)
    }
}

impl Drop for PowerSource {
    fn drop(&mut self) {
        unsafe { PowerSettingUnregisterNotification(self.registration) };
    }
}

unsafe extern "system" fn power_changed(_: *const c_void, _: u32, _: *const c_void) -> u32 {
    if let Some(event) = POWER_CHANGED.get() {
        unsafe { SetEvent(event.as_raw_handle()) };
    }
    ERROR_SUCCESS
}

pub fn reset_display_timer() -> Result<(), String> {
    if unsafe { SetThreadExecutionState(ES_DISPLAY_REQUIRED) } == 0 {
        return Err("Resetting the display idle timer failed.".into());
    }
    Ok(())
}

pub struct PowerRequests {
    handle: OwnedHandle,
    display: bool,
}

impl PowerRequests {
    pub fn new() -> Result<Self, String> {
        let reason = REASON_CONTEXT {
            Version: POWER_REQUEST_CONTEXT_VERSION,
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: windows_sys::w!("Prevent idle sleep").cast_mut(),
            },
        };
        let handle = unsafe { PowerCreateRequest(&reason) };
        check(handle != INVALID_HANDLE_VALUE, "Creating the power request")?;
        // SAFETY: PowerCreateRequest returned an owned handle.
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        check(
            unsafe { PowerSetRequest(handle.as_raw_handle(), PowerRequestSystemRequired) } != 0,
            "Setting the system power request",
        )?;
        Ok(Self {
            handle,
            display: false,
        })
    }

    pub fn set_display(&mut self, display: bool) -> Result<(), String> {
        if self.display != display {
            let handle = self.handle.as_raw_handle();
            let result = unsafe {
                if display {
                    PowerSetRequest(handle, PowerRequestDisplayRequired)
                } else {
                    PowerClearRequest(handle, PowerRequestDisplayRequired)
                }
            };
            check(result != 0, "Updating the display power request")?;
            self.display = display;
        }
        Ok(())
    }
}

fn check(success: bool, operation: &str) -> Result<(), String> {
    if !success {
        return Err(format!(
            "{operation} failed. {}",
            io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    #[test]
    #[ignore = "requires the Windows power notification service"]
    fn subscription_signals_the_initial_power_source() {
        let source = PowerSource::subscribe().unwrap();
        let result = unsafe { WaitForSingleObject(source.event().as_raw_handle(), 5_000) };
        assert_eq!(result, WAIT_OBJECT_0);
    }
}
