# Windows 11 release

DeskPet ships a 64-bit NSIS installer for Windows 11 build 22000 or newer.
The installer is per-user, so installing DeskPet itself does not require an
administrator account. It creates an entry in Windows Installed Apps and a
Start Menu shortcut under `DeskPet`.

## Build

On macOS, install the cross-build tools once:

```bash
brew install nsis llvm lld
cargo install cargo-xwin --locked
rustup target add x86_64-pc-windows-msvc
```

The public installer includes only `packs/default`. Private pet packs stay in
the user's application-data directory and are never bundled. Run:

```bash
./tools/build-windows-installer.sh
```

The output is:

```text
target/x86_64-pc-windows-msvc/release/bundle/nsis/DeskPet_0.1.0_x64-setup.exe
```

The build script compiles the Windows app, compiles and bundles the
`deskpet-assetpipe.exe` sidecar, includes the generic default pack, and runs
the static installer policy gate.

## Install and uninstall contract

- Windows 10 and earlier are rejected by the installer. Windows 11 build 22000+
  is accepted.
- Program files, the Start Menu shortcut and the Installed Apps registration
  are owned by the NSIS installer and removed by its uninstaller.
- User data lives under `%APPDATA%\app.deskpet` and is not removed. In
  particular, `%APPDATA%\app.deskpet\packs` survives uninstall/reinstall.
- A fresh install selects the bundled generic pack. Reinstalling never
  overwrites an existing config, active pack, sleep time or saved position.

## Signing

The local cross-built installer is unsigned. Windows SmartScreen may therefore
show an unknown-publisher warning. Public distribution should add an Authenticode
certificate and timestamping; it does not change the installer or data-retention
contract above.

## Validation

The `0.1.0` x64 installer was exercised end to end on Windows 11 ARM through
Windows x64 emulation on 2026-10-02:

- installation completed and launched the transparent default desktop window;
- the bundled default manifest and the generated user config were present;
- uninstall removed the install directory, executable, desktop shortcut and
  Installed Apps registration;
- `%APPDATA%\app.deskpet\config.toml` and a marker under
  `%APPDATA%\app.deskpet\packs` remained after uninstall.
