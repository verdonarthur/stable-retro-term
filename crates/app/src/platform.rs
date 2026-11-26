use std::error::Error;
use std::io::{self, Read};

use retro_renderer::{Instance, InstanceDescriptor};
use winit::event_loop::ActiveEventLoop;
use wl_clipboard_rs::{copy, paste};

pub(crate) fn graphics(loop_: &ActiveEventLoop) -> Instance {
    Instance::new(InstanceDescriptor::new_with_display_handle(Box::new(
        loop_.owned_display_handle(),
    )))
}

pub(crate) fn copy_text(text: String) -> Result<(), Box<dyn Error>> {
    copy::Options::new().copy(
        copy::Source::Bytes(text.into_bytes().into_boxed_slice()),
        copy::MimeType::Text,
    )?;
    Ok(())
}

pub(crate) fn paste_text() -> Result<String, Box<dyn Error>> {
    const MAX_BYTES: usize = 8 * 1024 * 1024;
    let (pipe, _) = paste::get_contents(
        paste::ClipboardType::Regular,
        paste::Seat::Unspecified,
        paste::MimeType::Text,
    )?;
    let mut text = String::new();
    pipe.take((MAX_BYTES + 1) as u64)
        .read_to_string(&mut text)?;
    if text.len() > MAX_BYTES {
        return Err(io::Error::other("clipboard text exceeds the paste limit").into());
    }
    Ok(text)
}

/// The user file path from the process environment.
pub fn config_path() -> Option<std::path::PathBuf> {
    crate::config::user_config_path(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}
