#!/usr/bin/env bash
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.."
tools=target/xtask/appimage-tools
mkdir -p "$tools"
scratch=$(mktemp -d "$tools/download.XXXXXXXX")
trap 'rm -rf -- "$scratch"' EXIT

download() {
    local url=$1 name=$2 checksum=$3
    curl -fsSL --retry 3 "$url" -o "$scratch/$name"
    (cd "$scratch" && echo "$checksum  $name" | sha256sum -c -)
}

tool=appimagetool-x86_64.AppImage
download "https://github.com/AppImage/appimagetool/releases/download/1.9.1/$tool" \
    "$tool" ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
download https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64 \
    runtime-x86_64 2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
download https://github.com/libfuse/libfuse/releases/download/fuse-3.15.0/fuse-3.15.0.tar.xz \
    fuse-3.15.0.tar.xz 70589cfd5e1cff7ccd6ac91c86c01be340b227285c5e200baa284e401eea2ca0
download https://github.com/vasi/squashfuse/archive/0.5.2.tar.gz \
    squashfuse-0.5.2.tar.gz db0238c5981dabbd80ee09ae15387f390091668ca060a7bc38047912491443d3

runtime_commit=dd6cebedcbddde9c82f89b011e8e1d40b6e43868
download "https://codeload.github.com/AppImage/type2-runtime/tar.gz/$runtime_commit" \
    type2-runtime.tar.gz f5fec23be76e50e2445ed2d018bac49b367490fc483c62c4637e99ec705d27ba
tar -xOf "$scratch/type2-runtime.tar.gz" "type2-runtime-$runtime_commit/LICENSE" \
    > "$scratch/runtime-LICENSE"
tar -xOf "$scratch/fuse-3.15.0.tar.xz" fuse-3.15.0/LGPL2.txt > "$scratch/libfuse-LGPL2.txt"
chmod +x "$scratch/$tool"
mv -- "$scratch"/* "$tools/"
