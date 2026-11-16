use windows::{
    Win32::{
        Media::Audio::{
            Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture,
            eConsole,
        },
        System::Com::{CLSCTX_ALL, CoCreateInstance},
    },
    core::Result,
};

// Resolve the current default input each time, including after plugging in a USB mic.
fn endpoint() -> Result<IAudioEndpointVolume> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        enumerator
            .GetDefaultAudioEndpoint(eCapture, eConsole)?
            .Activate(CLSCTX_ALL, None)
    }
}

pub fn muted() -> Result<bool> {
    unsafe { Ok(endpoint()?.GetMute()?.as_bool()) }
}

pub fn toggle() -> Result<bool> {
    unsafe {
        let volume = endpoint()?;
        let next = !volume.GetMute()?.as_bool();
        volume.SetMute(next, std::ptr::null())?;
        Ok(next)
    }
}

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, PROPERTYKEY, WPARAM},
        Media::Audio::{
            Endpoints::{IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl},
            *,
        },
        UI::WindowsAndMessaging::{PostMessageW, WM_APP},
    },
    core::{PCWSTR, implement},
};
pub const VOLUME_CHANGED: u32 = WM_APP + 5;
pub const DEVICE_CHANGED: u32 = WM_APP + 6;

fn post(hwnd: usize, message: u32) {
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), message, WPARAM(0), LPARAM(0));
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeListener {
    hwnd: usize,
}
impl IAudioEndpointVolumeCallback_Impl for VolumeListener_Impl {
    fn OnNotify(&self, _: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        post(self.hwnd, VOLUME_CHANGED);
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DeviceListener {
    hwnd: usize,
}
impl IMMNotificationClient_Impl for DeviceListener_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
        post(self.hwnd, DEVICE_CHANGED);
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> Result<()> {
        post(self.hwnd, DEVICE_CHANGED);
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> Result<()> {
        post(self.hwnd, DEVICE_CHANGED);
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> Result<()> {
        if flow == eCapture && role == eConsole {
            post(self.hwnd, DEVICE_CHANGED);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

// Only event callbacks run while idle: no timer or polling thread is needed.
pub struct Monitor {
    enumerator: IMMDeviceEnumerator,
    devices: IMMNotificationClient,
    listener: IAudioEndpointVolumeCallback,
    volume: Option<IAudioEndpointVolume>,
}
impl Monitor {
    pub fn new(hwnd: HWND) -> Result<Self> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let devices: IMMNotificationClient = DeviceListener {
                hwnd: hwnd.0 as usize,
            }
            .into();
            let listener: IAudioEndpointVolumeCallback = VolumeListener {
                hwnd: hwnd.0 as usize,
            }
            .into();
            enumerator.RegisterEndpointNotificationCallback(&devices)?;
            let mut monitor = Self {
                enumerator,
                devices,
                listener,
                volume: None,
            };
            // A missing mic is a normal state; the device listener can detect reconnection.
            let _ = monitor.rebind();
            Ok(monitor)
        }
    }
    pub fn rebind(&mut self) -> Result<()> {
        unsafe {
            if let Some(volume) = self.volume.take() {
                let _ = volume.UnregisterControlChangeNotify(&self.listener);
            }
            let volume = endpoint()?;
            volume.RegisterControlChangeNotify(&self.listener)?;
            self.volume = Some(volume);
            Ok(())
        }
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        unsafe {
            if let Some(volume) = self.volume.take() {
                let _ = volume.UnregisterControlChangeNotify(&self.listener);
            }
            let _ = self
                .enumerator
                .UnregisterEndpointNotificationCallback(&self.devices);
        }
    }
}
