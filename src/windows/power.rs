use std::ffi::c_void;
use std::io;
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
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

// A delayed callback looks up its event without dereferencing its context.
static SUBSCRIPTIONS: Mutex<Subscriptions> = Mutex::new(Subscriptions {
    next: 1,
    events: Vec::new(),
});

struct Subscriptions {
    next: usize,
    events: Vec<(usize, Arc<OwnedHandle>)>,
}

fn subscriptions() -> MutexGuard<'static, Subscriptions> {
    SUBSCRIPTIONS.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct PowerSource {
    registration: HPOWERNOTIFY,
    parameters: Option<Box<DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS>>,
    event: Arc<OwnedHandle>,
}

impl PowerSource {
    pub fn subscribe() -> io::Result<Self> {
        let handle = unsafe { CreateEventW(null(), 0, 0, null()) };
        check(!handle.is_null())?;
        // SAFETY: CreateEventW returned an owned handle.
        let event = Arc::new(unsafe { OwnedHandle::from_raw_handle(handle) });
        let id = {
            let mut subscriptions = subscriptions();
            let id = subscriptions.next;
            subscriptions.next = id
                .checked_add(1)
                .ok_or_else(|| io::Error::other("Power notification IDs exhausted."))?;
            subscriptions.events.push((id, Arc::clone(&event)));
            id
        };
        let mut parameters = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(power_changed),
            Context: std::ptr::without_provenance_mut(id),
        });
        let mut registration = null_mut();
        // SAFETY: The callback and its parameters outlive Win32's registration.
        let status = unsafe {
            PowerSettingRegisterNotification(
                &GUID_ACDC_POWER_SOURCE,
                DEVICE_NOTIFY_CALLBACK,
                std::ptr::from_mut(parameters.as_mut()).cast(),
                &mut registration,
            )
        };
        if status != ERROR_SUCCESS {
            remove_subscription(id);
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        Ok(Self {
            registration: registration as HPOWERNOTIFY,
            parameters: Some(parameters),
            event,
        })
    }

    pub fn event(&self) -> BorrowedHandle<'_> {
        self.event.as_handle()
    }

    pub fn on_ac(&self) -> io::Result<bool> {
        let mut status = SYSTEM_POWER_STATUS::default();
        check(unsafe { GetSystemPowerStatus(&mut status) } != 0)?;
        Ok(status.ACLineStatus == 1)
    }
}

impl Drop for PowerSource {
    fn drop(&mut self) {
        let status = unsafe { PowerSettingUnregisterNotification(self.registration) };
        if let Some(parameters) = self.parameters.take() {
            remove_subscription(parameters.Context.addr());
            if status != ERROR_SUCCESS {
                // Windows may still own the registration's pointer on failure.
                let _ = Box::leak(parameters);
            }
        }
    }
}

fn remove_subscription(id: usize) {
    let mut subscriptions = subscriptions();
    if let Some(index) = subscriptions.events.iter().position(|(key, _)| *key == id) {
        subscriptions.events.swap_remove(index);
    }
}

unsafe extern "system" fn power_changed(context: *const c_void, _: u32, _: *const c_void) -> u32 {
    let id = context.addr();
    if let Some((_, event)) = subscriptions().events.iter().find(|(key, _)| *key == id) {
        unsafe { SetEvent(event.as_raw_handle()) };
    }
    ERROR_SUCCESS
}

pub fn reset_display_timer() -> io::Result<()> {
    if unsafe { SetThreadExecutionState(ES_DISPLAY_REQUIRED) } == 0 {
        return Err(io::Error::other("Resetting the display idle timer failed."));
    }
    Ok(())
}

pub struct PowerRequests {
    handle: OwnedHandle,
    display: bool,
}

impl PowerRequests {
    pub fn new() -> io::Result<Self> {
        let reason = REASON_CONTEXT {
            Version: POWER_REQUEST_CONTEXT_VERSION,
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: windows_sys::w!("Prevent idle sleep").cast_mut(),
            },
        };
        let handle = unsafe { PowerCreateRequest(&reason) };
        check(handle != INVALID_HANDLE_VALUE)?;
        // SAFETY: PowerCreateRequest returned an owned handle.
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        check(unsafe { PowerSetRequest(handle.as_raw_handle(), PowerRequestSystemRequired) } != 0)?;
        Ok(Self {
            handle,
            display: false,
        })
    }

    pub fn set_display(&mut self, display: bool) -> io::Result<()> {
        if self.display != display {
            let handle = self.handle.as_raw_handle();
            let result = unsafe {
                if display {
                    PowerSetRequest(handle, PowerRequestDisplayRequired)
                } else {
                    PowerClearRequest(handle, PowerRequestDisplayRequired)
                }
            };
            check(result != 0)?;
            self.display = display;
        }
        Ok(())
    }
}

fn check(success: bool) -> io::Result<()> {
    if !success {
        return Err(io::Error::last_os_error());
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
        let first = PowerSource::subscribe().unwrap();
        let second = PowerSource::subscribe().unwrap();
        assert_ne!(
            first.event().as_raw_handle(),
            second.event().as_raw_handle()
        );
        for source in [first, second] {
            let result = unsafe { WaitForSingleObject(source.event().as_raw_handle(), 5_000) };
            assert_eq!(result, WAIT_OBJECT_0);
        }
    }
}
