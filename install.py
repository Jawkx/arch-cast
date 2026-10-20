#!/usr/bin/python3
"""User-local install. No sudo, package removal, or system files."""
from datetime import datetime
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
HOME = Path.home()
CONFIG = HOME / ".config/arch-cast"
BIN = HOME / ".local/bin/arch-cast"
DESKTOP = HOME / ".local/share/applications/arch-cast.desktop"
HYPR = HOME / ".config/hypr/hyprland.lua"
START = "-- BEGIN ARCH CAST"
END = "-- END ARCH CAST"
BLOCK = '''-- BEGIN ARCH CAST
-- Standalone launcher. Remove this block and restore local menu to use Rofi.
hl.window_rule({
    name = "arch-cast-launcher",
    match = { class = "^(arch-cast)$" },
    float = true,
    center = true,
    dim_around = true,
    animation = "popin 95%",
})
hl.on("hyprland.start", function()
    hl.exec_cmd("$HOME/.local/bin/arch-cast daemon")
end)
-- END ARCH CAST'''


def install():
    from archcast.core import write_json
    subprocess.run(["/usr/bin/python3", "-c", "from PyQt6 import QtWidgets, QtNetwork"], check=True)
    text = HYPR.read_text()
    menu_match = re.search(r"^local menu = .+$", text, re.M)
    if not menu_match or not any(word in menu_match.group(0) for word in ("rofi", "arch-cast")):
        raise RuntimeError("Expected a Rofi or Arch Cast local menu variable; no desktop changes made")
    backup_dir = CONFIG / "backups" / datetime.now().strftime("%Y%m%d-%H%M%S-%f")
    backup_dir.mkdir(parents=True)
    shutil.copy2(HYPR, backup_dir / "hyprland.lua")
    manifest = {"hypr_backup": str(backup_dir / "hyprland.lua"), "previous_menu": menu_match.group(0), "root": str(ROOT)}
    if BIN.exists() or BIN.is_symlink():
        if BIN.resolve() != (ROOT / "bin/arch-cast").resolve():
            shutil.copy2(BIN, backup_dir / "arch-cast.previous")
    BIN.parent.mkdir(parents=True, exist_ok=True)
    BIN.unlink(missing_ok=True)
    BIN.symlink_to(ROOT / "bin/arch-cast")
    (ROOT / "bin/arch-cast").chmod(0o755)
    DESKTOP.parent.mkdir(parents=True, exist_ok=True)
    DESKTOP.write_text(f'''[Desktop Entry]
Type=Application
Name=Arch Cast
Comment=Search apps and commands
Exec={BIN} show
Icon=system-search
Terminal=false
Categories=Utility;
StartupWMClass=arch-cast
''')
    if not (CONFIG / "config.json").exists():
        write_json(CONFIG / "config.json", {"theme": "Catppuccin Mocha", "colors": {}, "font": "Inter", "font_size": 12, "width": 840, "commands": []})
    # Preserve the first pre-install backup across reinstalls.
    if not (CONFIG / "install.json").exists():
        write_json(CONFIG / "install.json", manifest)
    text = re.sub(r"^local menu = .+$", "local menu = '$HOME/.local/bin/arch-cast toggle'", text, count=1, flags=re.M)
    text = re.sub(re.escape(START) + r".*?" + re.escape(END) + r"\n?", "", text, flags=re.S).rstrip()
    HYPR.write_text(text + "\n\n" + BLOCK + "\n")
    result = subprocess.run(["hyprctl", "reload"], capture_output=True, text=True)
    errors = subprocess.run(["hyprctl", "configerrors"], capture_output=True, text=True)
    if result.returncode or errors.stdout.strip():
        shutil.copy2(backup_dir / "hyprland.lua", HYPR)
        subprocess.run(["hyprctl", "reload"], check=False)
        raise RuntimeError("Hyprland rejected config; restored backup: " + errors.stdout + result.stderr)
    subprocess.run(["update-desktop-database", str(DESKTOP.parent)], check=False)
    subprocess.run([str(BIN), "show"], check=True)
    print(f"Installed {BIN}\nSuper+Space now opens Arch Cast.\nBackup: {backup_dir / 'hyprland.lua'}")


def uninstall():
    manifest_path = CONFIG / "install.json"
    if not manifest_path.exists():
        raise RuntimeError("No install manifest found; no changes made")
    manifest = json.loads(manifest_path.read_text())
    text = HYPR.read_text()
    # Restore only our changes, never overwrite later unrelated desktop edits.
    text = re.sub(re.escape(START) + r".*?" + re.escape(END) + r"\n?", "", text, flags=re.S)
    text = text.replace("local menu = '$HOME/.local/bin/arch-cast toggle'", manifest["previous_menu"])
    HYPR.write_text(text.rstrip() + "\n")
    subprocess.run([str(BIN), "quit"], check=False)
    BIN.unlink(missing_ok=True)
    previous = Path(manifest["hypr_backup"]).parent / "arch-cast.previous"
    if previous.exists():
        shutil.copy2(previous, BIN)
    DESKTOP.unlink(missing_ok=True)
    manifest_path.unlink()
    subprocess.run(["hyprctl", "reload"], check=False)
    print("Restored the previous launcher. Kept settings and backups in " + str(CONFIG))


if __name__ == "__main__":
    if "--uninstall" in sys.argv:
        uninstall()
    else:
        install()
