# <img src="assets/icon.svg" alt="" width="40" height="40" align="center"> run-with-other-gpu

[![Latest release](https://img.shields.io/github/v/release/amir9480/run-with-other-gpu)](https://github.com/amir9480/run-with-other-gpu/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/amir9480/run-with-other-gpu/total)](https://github.com/amir9480/run-with-other-gpu/releases)
[![Build & Release](https://github.com/amir9480/run-with-other-gpu/actions/workflows/release.yml/badge.svg)](https://github.com/amir9480/run-with-other-gpu/actions/workflows/release.yml)
![Platform: Windows 10/11](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078D6)

**[⬇ Download the latest release](https://github.com/amir9480/run-with-other-gpu/releases/latest)**

Windows-only tool that adds a **Run with other graphic** submenu to the Explorer
context menu of `.exe` files, with one entry per GPU role, for example
`Power saving (Intel(R) Arc(TM) Graphics)` and `High performance (NVIDIA ...)`.

If you pick the GPU the app already uses, the tool just launches the app.
Otherwise it:

1. saves the app's current per-app GPU preference
   (`HKCU\Software\Microsoft\DirectX\UserGpuPreferences`) to
   `%LOCALAPPDATA%\RunWithOtherGpu\state.json`;
2. writes the chosen `GpuPreference` and launches the app inside a Job Object;
3. waits in the background (with no window or taskbar icon) until every process
   in the app's process tree has exited, then restores the original value.

If the helper is killed or crashes, the next start of the tool restores the
original preference from `state.json`.

## Screenshots

The context menu on an `.exe`:

![Explorer context menu showing "Run with other graphic" with a Power saving and a High performance entry](assets/Screenshot-2.jpg)

The dialog shown when you run the tool directly:

![Run with other GPU dialog with Install and Remove buttons](assets/Screenshot-1.jpg)

## Download

Get `run-with-other-gpu.exe` from the
[releases page](https://github.com/amir9480/run-with-other-gpu/releases). It's a
single standalone exe; run it and click **Install Context Menu**.

It needs the [Microsoft Visual C++ Redistributable (x64)](https://aka.ms/vs/17/release/vc_redist.x64.exe),
which most PCs already have. If it's missing, the app shows a download link
instead of starting.

## Usage

```
run-with-other-gpu.exe                       dialog with Install / Remove buttons
run-with-other-gpu.exe --install             install the context menu
run-with-other-gpu.exe --remove              remove the context menu (alias: --uninstall)
run-with-other-gpu.exe --run <role> <exe>    power-saving | high-performance
```

Install copies the exe to `%LOCALAPPDATA%\RunWithOtherGpu\` and registers that
copy under HKCU, so no admin rights are needed. On Windows 11 the menu is under
**Show more options**. GPU names are captured at install time; click Install
again after a hardware or driver change.

## Building

Requirements:
- MSVC toolchain (Visual Studio Build Tools).
- LLVM, because libui's bindgen needs `libclang`: `winget install LLVM.LLVM`.

```
set LIBCLANG_PATH=C:\Program Files\LLVM\bin
cargo build --release
```

Local builds show `dev` as the version in the window title; release builds show
the tag.

## Releasing

Push a version tag; the [Build & Release](.github/workflows/release.yml) workflow
builds the exe and publishes it as a GitHub release:

```
git tag v1.0.0
git push origin v1.0.0
```

Pushes to `master`/`main` that touch the code only build. They keep the cargo
cache warm, because a tag build can't reuse another tag's cache.

## Icon

`assets/icon.svg` is the source. After editing it, regenerate `assets/icon.ico`
with ImageMagick (`winget install ImageMagick.ImageMagick`):

```
magick -background none -density 384 assets\icon.svg -define icon:auto-resize=256,64,48,32,24,20,16 assets\icon.ico
```

`build.rs` embeds it through `assets/app.rc`.

## Notes

- "Let Windows decide" (or no per-app preference) counts as **power saving**.
- An app whose manifest requires admin is started through UAC. Only that
  process is tracked, not its children, because a non-elevated helper can't
  put an elevated process in its Job Object.
- If you change the app's GPU preference in Settings while it is running,
  your change is kept and is not reverted.
