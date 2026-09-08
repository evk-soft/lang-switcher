//! Read-only evidence about the actual output and session; never adjusts user volume.
use windows::{
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Media::Audio::{
            Endpoints::IAudioEndpointVolume, IAudioClient, IMMDevice, ISimpleAudioVolume,
        },
        System::Com::{
            CLSCTX_ALL, CoTaskMemFree, STGM_READ,
            StructuredStorage::{PropVariantClear, PropVariantToString},
        },
    },
    core::Result,
};

fn endpoint_name(device: &IMMDevice) -> Result<String> {
    // SAFETY: live IMMDevice on its owning STA; the property store is read-only.
    let properties = unsafe { device.OpenPropertyStore(STGM_READ) }?;
    // SAFETY: SDK property key, live store; returned variant is cleared below on all paths.
    let mut value = unsafe { properties.GetValue(&PKEY_Device_FriendlyName) }?;
    let mut text = [0u16; 512];
    // SAFETY: initialized variant and writable UTF-16 buffer with generated length.
    let converted = unsafe { PropVariantToString(&value, &mut text) };
    // SAFETY: GetValue transferred ownership of this variant, cleared exactly once.
    let cleared = unsafe { PropVariantClear(&mut value) };
    converted?;
    cleared?;
    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    Ok(String::from_utf16_lossy(&text[..end]))
}

fn endpoint_id(device: &IMMDevice) -> Result<String> {
    // SAFETY: live device; GetId returns a NUL-terminated CoTaskMem allocation.
    let id = unsafe { device.GetId() }?;
    // SAFETY: GetId's owned string is valid until CoTaskMemFree below; copy before release.
    let text = unsafe { id.to_string() };
    // SAFETY: release exactly the GetId allocation, including on UTF-16 conversion failure.
    unsafe { CoTaskMemFree(Some(id.0.cast())) };
    Ok(text?)
}

pub(super) fn log_output(device: &IMMDevice, client: &IAudioClient) {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    let name = endpoint_name(device);
    let id = endpoint_id(device);
    // SAFETY: live device on this STA; activate only its read-only volume interface.
    let endpoint: Result<IAudioEndpointVolume> = unsafe { device.Activate(CLSCTX_ALL, None) };
    let endpoint_state = endpoint.and_then(|volume| {
        // SAFETY: live interface, scalar/BOOL outputs; neither call changes volume/mute.
        unsafe {
            Ok((
                volume.GetMasterVolumeLevelScalar()?,
                volume.GetMute()?.as_bool(),
            ))
        }
    });
    // SAFETY: initialized shared-mode client; service used and released on the same STA.
    let session: Result<ISimpleAudioVolume> = unsafe { client.GetService() };
    let session_state = session.and_then(|volume| {
        // SAFETY: live session service; getters only, released before the audio client.
        unsafe { Ok((volume.GetMasterVolume()?, volume.GetMute()?.as_bool())) }
    });
    tracing::debug!(
        role = "console",
        endpoint_name = ?name,
        endpoint_id = ?id,
        endpoint_volume_mute = ?endpoint_state,
        session_volume_mute = ?session_state,
        "audio output state"
    );
}
