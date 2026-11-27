# AppImage runtime source

The Stable Retro Term AppImage embeds the AppImage Type 2 runtime. The runtime
statically links libfuse under the GNU Lesser General Public License. See
`runtime-LICENSE` and `libfuse-LGPL2.txt`.

Extract `type2-runtime.tar.gz`, enter its top-level directory, and run
`bash scripts/docker/build-with-docker.sh` on x86_64 with Docker. The build
writes `out/runtime-x86_64`. To relink with changes, use the supplied dependency
archives in `scripts/common/install-dependencies.sh`, apply your changes after
extraction, and rebuild with the supplied scripts.

To rebuild the AppImage with the resulting runtime, extract the AppDir with
`./retro-term-vVERSION-x86_64.AppImage --appimage-extract` and pass the resulting
`squashfs-root` and the rebuilt runtime to appimagetool:

```sh
ARCH=x86_64 appimagetool --no-appstream --runtime-file ./runtime-x86_64 \
    squashfs-root ./retro-term-relinked.AppImage
```
