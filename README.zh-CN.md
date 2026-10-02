# DeskPet

<p align="center">
  <a href="./README.md">English</a> |
  <a href="./README.zh-CN.md"><strong>简体中文</strong></a>
</p>

DeskPet 是一款支持 macOS 12+ 和 Windows 11 的真实质感桌面宠物应用。它使用透明动画、逐像素点击区域和桌面接触阴影，让宠物自然地待在桌面上，同时避免遮挡透明区域或抢占当前窗口焦点。

你可以上传自己的宠物图片或短视频，分别配置日常陪伴、睡眠、进入睡眠和醒来起身等状态，也可以创建“奔跑”“跳跃”等自定义动作并绑定单击或双击。长时间没有互动时，宠物会自动进入睡眠；再次互动后，它会通过对应的过渡动画回到日常陪伴状态。

宠物素材、生成的素材包和个人设置默认保存在本机。应用卸载后，用户导入的素材包可以继续保留，方便重新安装后继续使用。

## 主要功能

- 上传 PNG、JPEG、WebP、MP4 或 MOV 素材。
- 自动抠图、清理边缘、统一尺寸与位置，并生成桌面宠物素材包。
- 配置日常陪伴、睡眠以及双向过渡动画。
- 创建并命名单击和双击动作。
- 拖动宠物、穿透透明像素，并根据每一帧自动调整互动边界。
- 设置睡眠等待时间和是否开机自启。
- 一套代码同时支持 macOS 与 Windows。

DeskPet 使用 MIT 许可证。

## 使用方法

1. 从 [GitHub 最新版本](https://github.com/DannyYu2010/DeskPet/releases/latest)下载对应系统的安装包，安装并启动 DeskPet。
2. 点击菜单栏或系统托盘中的 DeskPet 图标，选择“设置”。
3. 在“固定模式素材”中先上传“日常陪伴”。这是必需素材，也是宠物平时显示的循环动作。
4. 根据需要上传“睡眠模式”“进入睡眠”和“醒来起身”。如果没有上传过渡动画，DeskPet 会直接切换到已有状态。
5. 添加互动动作时，先填写动作名称，再选择“单击”或“双击”，然后上传对应图片或短视频。
6. 设置多久不互动后进入睡眠，并根据需要开启“登录系统后自动启动”。
7. 点击“设为我的桌面宠物”。宠物出现后，可以直接拖动到桌面的任意位置。

支持导入 PNG、JPEG、WebP、MP4 和 MOV。短视频应控制在 10 秒以内。宠物素材和生成的素材包默认保存在本机。

macOS 安装包目前尚未经过 Apple 公证。如果系统首次启动时拦截 DeskPet，请在“应用程序”中按住 Control 键点按 DeskPet，选择“打开”并确认一次。

## 项目目标

- **自然融入桌面。** 使用接触阴影、逐像素点击穿透和视线跟踪，不抢占窗口焦点。
- **使用自己的角色。** 上传图片或视频后，素材流水线会自动抠图、清理边缘、对齐锚点并生成素材包。
- **安静陪伴。** 尽量降低待机 CPU 和内存占用，不抢焦点，并在全屏应用运行时隐藏。
- **一套代码。** macOS 与 Windows 共享绝大部分代码。

## 开发环境

需要安装 Xcode Command Line Tools（macOS）、Node.js 和 Rust。如果已经安装，可以直接前往[快速开始](#快速开始)。

### macOS

**1. Xcode Command Line Tools** — 提供 Rust 编译所需的 C 编译器和链接器。

```bash
xcode-select --install
```

**2. Homebrew** — 如果 `brew --version` 已经可用，可以跳过。

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

安装完成后，终端会提示两条写入 `~/.zprofile` 的命令，请执行它们，否则新终端可能找不到 `brew`。

**3. Node.js**

```bash
brew install node
```

**4. Rust**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

接受默认选项，然后执行 `source ~/.cargo/env`，或者重新打开终端。

### Windows 11

**1. Visual Studio Build Tools** — 从 [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) 安装“使用 C++ 的桌面开发”工作负载，Rust 需要使用 MSVC 链接器。

**2. Node.js** — 从 [Node.js 官网](https://nodejs.org/)安装 LTS 版本，或者运行：

```powershell
winget install OpenJS.NodeJS.LTS
```

**3. Rust** — 从 [rustup.rs](https://rustup.rs/)安装，或者运行：

```powershell
winget install Rustlang.Rustup
```

**4. WebView2** — Windows 11 已经自带，无需额外安装。

### 检查环境

```bash
node -v && npm -v && cargo --version
```

如果显示三个版本号，说明环境已经准备好。如果出现 `command not found`，请先重新打开终端，因为安装程序对环境变量的修改不会自动进入已有的终端会话。

## 快速开始

```bash
cd apps/desktop
npm install
npm run tauri dev
```

第一次构建需要编译数百个 Rust 依赖，通常需要 5–10 分钟。后续构建会快很多。

### 第一次运行失败时

- **`command not found: npm` 或 `cargo`** — 安装程序修改了环境变量，但当前终端尚未刷新，请重新打开终端。
- **链接器错误或找不到 `cc`** — macOS 缺少 Command Line Tools，或者 Windows 缺少 Build Tools。
- **`platform/macos.rs` 出现 Rust 编译错误** — 可能是 `objc2` API 签名发生变化，请提交 Issue 并附上错误信息。
- **宠物周围出现白色方框** — 透明窗口没有正确生效，这是需要报告的程序问题。

## 通知接口

DeskPet 不会读取其他应用的系统通知。它会监听本机地址，任何工具都可以主动向它发送通知：

```bash
curl -X POST http://127.0.0.1:7423/notify \
     -H "Authorization: Bearer $(cat ~/.config/deskpet/token)" \
     -H "Content-Type: application/json" \
     -d '{"title":"Build passed","body":"main @ a1b2c3","source":"ci"}'
```

相关设计说明参见 [ARCHITECTURE.md](docs/ARCHITECTURE.md)。

## 制作素材包

请参阅 [PETPACK_SPEC.md](docs/PETPACK_SPEC.md)。素材包是包含 `manifest.json` 和 WebP 帧的 zip 文件。只有一张静态 PNG 也可以成为有效素材包，运行时会在此基础上增加呼吸和眨眼效果。

素材包负责声明外观，运行时负责行为。这样，运行时行为改进可以自动应用到已有素材包。

## 与开发代理协作

Claude、Codex 和 WorkBuddy 都可能参与这个仓库的开发。Memory Hub 项目未接入时，以 **Git 作为共享记录来源**。

- 首先阅读 [`docs/STATE.md`](docs/STATE.md)，了解当前进度和下一步工作。
- 阅读 [`docs/HANDOFF.md`](docs/HANDOFF.md)，了解已确定的设计和已知问题。
- 开发约定参见 [`docs/MEMORY.md`](docs/MEMORY.md)。

修改代码时，请在同一个提交中更新 `STATE.md`。

## 参与贡献

最重要的规则：

> `#[cfg(target_os = ...)]` 只能出现在 `src-tauri/src/platform/` 中，不能放在 `core/` 或 TypeScript 代码里。

CI 会检查这条规则。如果其他模块需要平台特定行为，请在 `PetWindow` trait 中增加方法，并分别实现 macOS 和 Windows 版本。

项目优先在 macOS 上开发，但每个 PR 都必须保证两个平台能够通过编译和静态检查。

## 已知限制

- macOS 透明窗口需要启用 Tauri 的 `macOSPrivateApi`，因此无法通过 Mac App Store 发布，目前采用独立分发。
- macOS 正式分发需要 Apple Developer 账户完成签名和公证。没有公证时，用户首次启动需要右键点击应用并选择“打开”。
- macOS 不支持读取系统通知；Windows 版本也尚未实现该功能，因为它需要 MSIX 软件包身份。

## 许可证

代码使用 MIT 许可证，仓库内置素材包均为原创或采用 CC0 许可。

模型权重会在首次使用时下载，不会提交到仓库。贡献者请注意：**RMBG-2.0 使用 CC BY-NC 许可证，不能用于这里**。本项目使用 BiRefNet（MIT）进行抠图，并使用 FILM（Apache-2.0）进行插帧。添加模型前请检查许可证，许多常用模型带有与本项目不兼容的非商业限制。
