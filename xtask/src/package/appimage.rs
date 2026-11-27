use std::fs::{self, Permissions};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

use crate::package::sha256_file;
use crate::process::{artifacts_dir, find_in_path, output, run_logged};
use crate::report::{Status, Step};

const TOOL: &str = "appimagetool-x86_64.AppImage";
const RUNTIME: &str = "runtime-x86_64";
const SOURCE_FILES: [&str; 5] = [
    "type2-runtime.tar.gz",
    "fuse-3.15.0.tar.xz",
    "squashfuse-0.5.2.tar.gz",
    "runtime-LICENSE",
    "libfuse-LGPL2.txt",
];

pub(super) fn build(
    root: &Path,
    distrib: &Path,
    binary: &Path,
    binary_sha: &str,
    version: &str,
) -> Step {
    let tools = artifacts_dir(root).join("appimage-tools");
    for file in [TOOL, RUNTIME].into_iter().chain(SOURCE_FILES) {
        if !tools.join(file).is_file() {
            return Step::new("appimage", Status::Missing).with_note(format!(
                "missing {file}; run bash packaging/appimage/fetch-tools.sh"
            ));
        }
    }
    if find_in_path("desktop-file-validate").is_none() {
        return Step::new("appimage", Status::Missing)
            .with_note("install desktop-file-utils for desktop-file-validate");
    }

    let log = artifacts_dir(root).join("logs/package-appimage.log");
    match assemble(root, distrib, binary, binary_sha, version, &tools, &log) {
        Ok(()) => Step::new("appimage", Status::Passed),
        Err(message) => {
            let mut step = Step::new("appimage", Status::Failed).with_note(message.to_string());
            step.log = Some(log);
            step
        }
    }
}

fn assemble(
    root: &Path,
    distrib: &Path,
    binary: &Path,
    binary_sha: &str,
    version: &str,
    tools: &Path,
    log: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let scratch = artifacts_dir(root).join("package/appimage");
    if scratch.exists() {
        fs::remove_dir_all(&scratch)?;
    }
    let appdir = scratch.join("RetroTerm.AppDir");
    let docs = appdir.join("usr/share/doc/retro-term");
    let sources = scratch.join("runtime-source");
    let assets = root.join("packaging/appimage");
    fs::create_dir_all(appdir.join("usr/bin"))?;
    fs::create_dir_all(&docs)?;
    fs::create_dir(&sources)?;
    for (source, dest, mode) in [
        (
            binary.to_path_buf(),
            appdir.join("usr/bin/retro-term"),
            0o755,
        ),
        (assets.join("AppRun"), appdir.join("AppRun"), 0o755),
        (
            assets.join("retro-term.desktop"),
            appdir.join("retro-term.desktop"),
            0o644,
        ),
        (
            assets.join("retro-term.png"),
            appdir.join("retro-term.png"),
            0o644,
        ),
        (root.join("LICENSE"), docs.join("LICENSE"), 0o644),
        (
            assets.join("runtime-source.md"),
            docs.join("runtime-source.md"),
            0o644,
        ),
        (
            assets.join("runtime-source.md"),
            sources.join("README.md"),
            0o644,
        ),
    ] {
        copy(&source, &dest, mode)?;
    }
    symlink("retro-term.png", appdir.join(".DirIcon"))?;
    for file in SOURCE_FILES {
        copy(&tools.join(file), &sources.join(file), 0o644)?;
    }
    for notice in ["runtime-LICENSE", "libfuse-LGPL2.txt"] {
        copy(&tools.join(notice), &docs.join(notice), 0o644)?;
    }
    output(
        "desktop-file-validate",
        &["RetroTerm.AppDir/retro-term.desktop"],
        &scratch,
    )
    .map_err(|failure| failure.message)?;
    let epoch = output("git", &["show", "-s", "--format=%ct", "HEAD"], root)
        .map_err(|failure| failure.message)?;
    let timestamp = format!("@{epoch}");
    output(
        "find",
        &[
            "RetroTerm.AppDir",
            "-exec",
            "touch",
            "-h",
            "-d",
            &timestamp,
            "{}",
            "+",
        ],
        &scratch,
    )
    .map_err(|failure| failure.message)?;

    let (image, source_archive) = asset_names(version);
    let tool = tools.join(TOOL);
    let runtime = tools.join(RUNTIME);
    // Cargo adds its build library paths when it launches xtask. The AppImage
    // tools and the packaged executable should resolve their own libraries.
    let exit = run_logged(
        "env",
        &[
            "-u",
            "LD_LIBRARY_PATH",
            "APPIMAGE_EXTRACT_AND_RUN=1",
            "ARCH=x86_64",
            &format!("SOURCE_DATE_EPOCH={epoch}"),
            tool.to_str().ok_or("appimagetool path must be UTF-8")?,
            "--no-appstream",
            "--runtime-file",
            runtime.to_str().ok_or("runtime path must be UTF-8")?,
            "RetroTerm.AppDir",
            &image,
        ],
        &scratch,
        log,
    )
    .map_err(|failure| failure.message)?;
    if exit != Some(0) {
        return Err("appimagetool failed".into());
    }
    let executable = format!("./{image}");
    let help = output(
        "env",
        &[
            "-u",
            "LD_LIBRARY_PATH",
            "APPIMAGE_EXTRACT_AND_RUN=1",
            &executable,
            "--help",
        ],
        &scratch,
    )
    .map_err(|failure| failure.message)?;
    if !help.starts_with("Usage: retro-term") {
        return Err(format!("unexpected AppImage --help output: {help}").into());
    }
    output(
        "env",
        &[
            "-u",
            "LD_LIBRARY_PATH",
            &executable,
            "--appimage-extract",
            "usr/bin/retro-term",
        ],
        &scratch,
    )
    .map_err(|failure| failure.message)?;
    let payload = scratch.join("squashfs-root/usr/bin/retro-term");
    if sha256_file(&payload).map_err(|failure| failure.message)? != binary_sha {
        return Err("AppImage payload SHA-256 differs from the archive binary".into());
    }

    output(
        "tar",
        &[
            "--sort=name",
            "--mtime",
            &timestamp,
            "--owner=0",
            "--group=0",
            "--numeric-owner",
            "--mode=u+rwX,go+rX,go-w",
            "-cJf",
            &source_archive,
            "runtime-source",
        ],
        &scratch,
    )
    .map_err(|failure| failure.message)?;
    for name in [&image, &source_archive] {
        let checksum = write_checksum(&scratch, name)?;
        fs::rename(scratch.join(name), distrib.join(name))?;
        fs::rename(scratch.join(&checksum), distrib.join(checksum))?;
    }
    Ok(())
}

fn copy(source: &Path, dest: &Path, mode: u32) -> std::io::Result<()> {
    fs::copy(source, dest)?;
    fs::set_permissions(dest, Permissions::from_mode(mode))
}

fn asset_names(version: &str) -> (String, String) {
    (
        format!("retro-term-v{version}-x86_64.AppImage"),
        format!("retro-term-v{version}-appimage-runtime-source.tar.xz"),
    )
}

fn write_checksum(dir: &Path, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let digest = sha256_file(&dir.join(name)).map_err(|failure| failure.message)?;
    let checksum = format!("{name}.sha256");
    fs::write(dir.join(&checksum), format!("{digest}  {name}\n"))?;
    Ok(checksum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_include_the_version() {
        let (image, source) = asset_names("1.2.3");
        assert_eq!(image, "retro-term-v1.2.3-x86_64.AppImage");
        assert_eq!(source, "retro-term-v1.2.3-appimage-runtime-source.tar.xz");
    }

    #[test]
    fn checksum_uses_the_asset_basename() {
        let dir = tempfile::tempdir().unwrap();
        let (name, _) = asset_names("1.2.3");
        fs::write(dir.path().join(&name), "abc").unwrap();
        let checksum = write_checksum(dir.path(), &name).unwrap();
        assert_eq!(checksum, format!("{name}.sha256"));
        assert_eq!(
            fs::read_to_string(dir.path().join(checksum)).unwrap(),
            format!("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  {name}\n")
        );
    }
}
