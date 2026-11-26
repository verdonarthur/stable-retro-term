//! Application shell: windows, input, and configuration.

#[cfg(target_os = "linux")]
mod app;
pub mod config;
#[cfg(target_os = "linux")]
mod input;
#[cfg(target_os = "linux")]
mod platform;
#[cfg(target_os = "linux")]
mod scene;
#[cfg(target_os = "linux")]
mod scheduler;

#[cfg(target_os = "linux")]
pub use app::{run, run_with_config, run_with_mode, run_with_program};
#[cfg(target_os = "linux")]
pub use platform::config_path;
