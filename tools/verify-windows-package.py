#!/usr/bin/env python3
"""Static release gate for the Windows 11 NSIS package configuration."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TAURI = ROOT / "apps/desktop/src-tauri/tauri.conf.json"
HOOKS = ROOT / "apps/desktop/src-tauri/windows/nsis-hooks.nsh"
PACK = ROOT / "packs/default/manifest.json"
DEFAULT_CONFIG = ROOT / "apps/desktop/src-tauri/windows/default-config.toml"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"FAILED: {message}")


config = json.loads(TAURI.read_text(encoding="utf-8"))
windows = config["bundle"]["windows"]
nsis = windows["nsis"]
hooks = HOOKS.read_text(encoding="utf-8")
hook_commands = "\n".join(
    line for line in hooks.splitlines() if not line.lstrip().startswith(";")
)

require("nsis" in config["bundle"]["targets"], "NSIS is not a bundle target")
require(nsis["installMode"] == "currentUser", "installer must not require admin")
require(nsis["startMenuFolder"] == "DeskPet", "Start Menu shortcut is not configured")
require(nsis["installerHooks"] == "windows/nsis-hooks.nsh", "installer hooks are not enabled")
require("CurrentBuildNumber" in hooks and "22000" in hooks, "Windows 11 build gate is missing")
require("$APPDATA\\app.deskpet\\packs" in hooks, "pack retention policy is undocumented")
require("NSIS_HOOK_POSTINSTALL" in hooks, "fresh-install configuration hook is missing")
require('active_pack = "default"' in DEFAULT_CONFIG.read_text(encoding="utf-8"), "generic pack is not the fresh-install default")
require(
    "RMDir" not in hook_commands and "Delete $APPDATA" not in hook_commands,
    "uninstaller removes user data",
)
require(PACK.is_file(), "the generic default pack is absent from this release build")

manifest = json.loads(PACK.read_text(encoding="utf-8"))
require(manifest.get("id") == "app.deskpet.default", "unexpected default pack identity")
require((PACK.parent / "sprites").is_dir(), "default sprite directory is missing")

print("Windows package policy: OK")
print("- Windows 11 build 22000+ enforced")
print("- current-user install and Start Menu registration enabled")
print("- uninstall preserves user packs under %APPDATA%\\app.deskpet\\packs")
print("- generic default pack included; private pet packs are excluded")
