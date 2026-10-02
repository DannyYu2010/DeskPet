# DeskPet

## 中文介绍

DeskPet 是一款支持 macOS 12+ 和 Windows 11 的真实质感桌面宠物应用。它使用透明动画、逐像素点击区域和桌面接触阴影，让宠物自然地待在桌面上，同时避免遮挡透明区域或抢占当前窗口焦点。

你可以上传自己的宠物图片或短视频，分别配置日常陪伴、睡眠、进入睡眠和醒来起身等状态，也可以创建“奔跑”“跳跃”等自定义动作并绑定单击或双击。长时间没有互动时，宠物会自动进入睡眠；再次互动后，它会通过对应的过渡动画回到日常陪伴状态。

宠物素材、生成的素材包和个人设置优先保存在本机。应用卸载后，用户导入的素材包可以继续保留，方便重新安装后继续使用。

主要功能：

- 上传 PNG、JPEG、WebP、MP4 或 MOV 素材。
- 自动抠图、清理边缘、统一尺寸与位置，并生成桌面宠物素材包。
- 配置日常陪伴、睡眠以及双向过渡动画。
- 自定义单击和双击动作。
- 支持拖动宠物、透明区域点击穿透和按当前动作自动收缩互动边界。
- 支持设置睡眠等待时间和是否开机自启。
- 一套代码同时支持 macOS 与 Windows。

## English Introduction

DeskPet is a lifelike desktop-pet application for macOS 12+ and Windows 11. Transparent animation, per-pixel hit testing, and contact shadows make the pet feel attached to the desktop while keeping transparent areas click-through and avoiding focus stealing.

Users can import pet images or short videos and assign them to daily companionship, sleep, lie-down, and wake-up states. Named actions such as running or jumping can be bound to a single or double click. After a period without interaction, the pet enters sleep and returns through the configured wake-up transition when the user interacts again.

Pet media, generated packs, and personal settings are stored locally by default. User-imported packs can remain available after uninstalling the application, ready for a later reinstall.

Key features:

- Import PNG, JPEG, WebP, MP4, and MOV media.
- Automatically matte, clean, align, normalize, and package imported material.
- Configure daily, sleep, lie-down, and wake-up animations.
- Create named actions for single and double clicks.
- Drag the pet, click through transparent pixels, and adapt interaction bounds to every frame.
- Configure sleep timing and launch at login.
- Share one codebase across macOS and Windows.

DeskPet is MIT licensed.

## Goals

- **Looks attached to the desktop.** Contact shadows, per-pixel click-through, gaze tracking, no focus stealing.
- **Bring your own character.** Upload a few images; the asset pipeline mattes, cleans edges, aligns anchors and packs them.
- **Quiet.** Under 0.5% idle CPU and 60 MB resident. Never steals focus, hides during fullscreen apps, respects Do Not Disturb.
- **One codebase.** ~85-90% shared between macOS and Windows.

## Prerequisites

Three things: Xcode Command Line Tools (macOS), Node.js, and Rust. If you
already have all three, skip to [Quick start](#quick-start).

### macOS

**1. Xcode Command Line Tools** — provides the C compiler and linker Rust needs.

```bash
xcode-select --install
```

**2. Homebrew** — skip if `brew --version` already works.

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

The installer prints two `echo ... >> ~/.zprofile` lines when it finishes.
**Run them.** Without that, `brew` won't be found in a new terminal.

**3. Node.js**

```bash
brew install node
```

**4. Rust**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Accept the defaults. Then `source ~/.cargo/env`, or just open a new terminal.

### Windows 11

**1. Visual Studio Build Tools** — install the "Desktop development with C++"
workload from <https://visualstudio.microsoft.com/visual-cpp-build-tools/>.
Rust uses the MSVC linker.

**2. Node.js** — <https://nodejs.org/> (LTS), or `winget install OpenJS.NodeJS.LTS`

**3. Rust** — <https://rustup.rs/>, or `winget install Rustlang.Rustup`

**4. WebView2** — already present on Windows 11. Nothing to do.

### Verify

```bash
node -v && npm -v && cargo --version
```

Three version numbers means you're set. A `command not found` means that step
didn't take effect — open a new terminal first, since installers modify your
shell profile and existing sessions don't pick that up.

## Quick start

```bash
cd apps/desktop
npm install
npm run tauri dev
```

The first build compiles several hundred Rust dependencies and takes roughly
5–10 minutes. It is not stuck. Later builds are seconds.

### If the first run fails

- **`command not found: npm` / `cargo`** — the installer edited your shell
  profile but this session predates it. Open a new terminal.
- **linker errors, or `cc` not found** — Command Line Tools (macOS) or Build
  Tools (Windows) are missing. See Prerequisites.
- **Rust compile errors in `platform/macos.rs`** — likely an `objc2` API
  signature change. Open an issue with the error text.
- **A white box around the pet** — window transparency failed. This is a real
  bug, not a config problem; please report it.

## Notifications

DeskPet does not read your other apps' notifications. It listens on localhost instead, so anything can push to it:

```bash
curl -X POST http://127.0.0.1:7423/notify \
     -H "Authorization: Bearer $(cat ~/.config/deskpet/token)" \
     -H "Content-Type: application/json" \
     -d '{"title":"Build passed","body":"main @ a1b2c3","source":"ci"}'
```

This is a deliberate scope cut, explained in [ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Making a pack

See [PETPACK_SPEC.md](docs/PETPACK_SPEC.md). A pack is a zip with a `manifest.json` and some WebP frames. A single static PNG is a valid pack — the runtime adds breathing and blinking on top.

Packs declare appearance; the runtime owns behaviour. That means behaviour improvements apply to every existing pack at once.

## Working on this with agents

Claude, Codex and WorkBuddy all touch this repo. Until a Memory Hub project
exists, **git is the shared memory layer**.

- Read [`docs/STATE.md`](docs/STATE.md) first — what's in progress, what's next.
- Read [`docs/HANDOFF.md`](docs/HANDOFF.md) — locked decisions and known gaps.
- Conventions in [`docs/MEMORY.md`](docs/MEMORY.md).

Update `STATE.md` in the same commit as your code.

## Contributing

One rule matters more than the rest:

> `#[cfg(target_os = ...)]` belongs **only** in `src-tauri/src/platform/`. Never in `core/`, never in TypeScript.

CI enforces this. If you need platform-specific behaviour elsewhere, add a method to the `PetWindow` trait and implement it on both sides.

Development is macOS-first, but both platforms must compile and lint clean on every PR.

## Known constraints

- Transparent windows on macOS require Tauri's `macOSPrivateApi`, which makes the app **ineligible for the Mac App Store**. We self-distribute.
- macOS distribution needs an Apple Developer account ($99/yr) for signing and notarization. Without it, users must right-click → Open on first launch.
- Reading system notifications is not supported on macOS (impossible post-SIP) and not yet implemented on Windows (needs MSIX package identity).

## Licenses

Code is MIT. Bundled packs are original work or CC0.

Model weights are downloaded on first use, not vendored. Note for contributors: **RMBG-2.0 is CC BY-NC and cannot be used here** — we use BiRefNet (MIT) for matting and FILM (Apache-2.0) for interpolation. Check the license of any model before adding it; several popular ones carry non-commercial clauses that are incompatible with this project.
