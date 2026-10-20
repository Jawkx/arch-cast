# Arch Cast

A standalone, native launcher for Arch Linux and Hyprland, inspired by [oma-cast](https://github.com/typecraft-dev/oma-cast). No Omarchy shell, plugins, menu actions, telemetry, Electron, or network services. Search stays local until you explicitly choose a web search.

Press **Super+Space** to toggle it. The default theme is **Catppuccin Mocha**.

![Arch Cast on this desktop](docs/screenshots/mocha.png)

[Desktop verification and screenshots](docs/verification.md) cover the shortcut, native app launch, action panel, calculator, and saved theme changes.

## What works

- Installed application search, including user overrides and Chrome web apps.
- Fuzzy name matching, keywords, aliases, learned searches, and frequency/recency ranking.
- Favorites, with initial shortcuts for Chrome, Ghostty, Steam, and WhatsApp Web when installed.
- Searchable Ctrl+K action panel with favorites, aliases, copying, ranking reset, and hidden results.
- Google and DuckDuckGo fallback searches. Queries are URL-encoded, never run as shell commands.
- Local arithmetic such as `12 * (3 + 4)` or `2^8`. Enter copies the answer.
- Theme settings with preset switching, individual color pickers, preview, font size, window width, and hidden-result restoration.
- Explicit, user-authored custom shell commands in the config file.
- One resident native process and a user-only local socket. No duplicate launcher windows.

## Keyboard

| Key | Action |
| --- | --- |
| Super+Space | Toggle launcher |
| Enter | Open selected result or copy calculator answer |
| Up / Down | Move selection |
| Ctrl+P / Ctrl+N | Move selection |
| Ctrl+Up / Ctrl+Down | Jump sections |
| Ctrl+1 through Ctrl+9 | Open numbered result |
| Ctrl+K | Toggle searchable action panel |
| Ctrl+Shift+F | Toggle favorite |
| Ctrl+Shift+C | Copy desktop ID or custom command |
| Alt+Shift+D | Hide selected application or custom command |
| Ctrl+, | Open settings |
| Up on empty search | Recall most recent search |
| Ctrl+Plus / Ctrl+Minus / Ctrl+0 | Increase, decrease, reset font size |
| Esc | Close actions, then clear search, then dismiss |
| Shift+Esc | Dismiss immediately |

Clicking outside also dismisses the launcher. App launches detach from the launcher, so closing or restarting it does not close your apps.

## Theme customization

Click **Settings** in the footer or press **Ctrl+,**. Choose Catppuccin Mocha, Macchiato, Latte, or Tokyo Night. Change any individual hex color with its picker. Preview applies without saving. Save persists changes; Cancel restores the previous appearance.

Settings live in `~/.config/arch-cast/config.json`:

```json
{
  "theme": "Catppuccin Mocha",
  "colors": {
    "accent": "#cba6f7"
  },
  "font": "Noto Sans",
  "font_size": 12,
  "width": 840,
  "commands": []
}
```

Available colors are `background`, `surface`, `selection`, `border`, `text`, `muted`, `accent`, and `success`. Six-digit `#rrggbb` values override the selected preset. Missing or invalid colors fall back to the preset. Edits load the next time the launcher opens. Use an empty `colors` object to use the preset unchanged.

Favorites, aliases, hidden entries, usage, and search history are stored separately in `~/.local/state/arch-cast/state.json`. Files are written atomically. Clearing this file resets personal ranking and favorites without changing your theme.

## Custom commands

Add a command in `config.json`, then reopen the launcher or run Refresh Applications:

```json
{
  "theme": "Catppuccin Mocha",
  "commands": [
    {
      "id": "screenshots",
      "name": "Open Screenshots",
      "description": "Pictures / Screenshots",
      "icon": "folder-pictures",
      "keywords": "captures images",
      "command": "xdg-open \"$HOME/Pictures/Screenshots\""
    }
  ]
}
```

Custom commands intentionally run through `/bin/sh`. Only add commands you trust. Search text is never interpolated into these commands.

## CLI

```sh
arch-cast toggle
arch-cast show
arch-cast hide
arch-cast quit
arch-cast doctor
arch-cast list ghostty
arch-cast run app:com.mitchellh.ghostty.desktop
```

The first invocation starts the resident in the background. `arch-cast daemon` runs it in the foreground for session startup or debugging. Logs are in `~/.local/state/arch-cast/launcher.log`. If you edit Python code, run `arch-cast quit` and reopen it.

## Install and restore Rofi

Requires Hyprland 0.56.2's Lua configuration API, `/usr/bin/python3`, `python-pyqt6`, `qt6-wayland`, `gtk3` for `gtk-launch`, and `xdg-utils`. The installer expects `~/.config/hypr/hyprland.lua` with a `local menu` variable pointing to Rofi. It does not support older Hyprlang configurations. Optional icon themes supply application artwork. The launcher works without a build step or pip environment.

```sh
git clone https://github.com/Jawkx/arch-cast.git
cd arch-cast
/usr/bin/python3 install.py
```

Review `install.py` before running it. It modifies your launcher binding and reloads Hyprland.

The installer:

1. Backs up `~/.config/hypr/hyprland.lua` under `~/.config/arch-cast/backups/`.
2. Links `~/.local/bin/arch-cast` to this project's executable and adds a desktop entry.
3. Changes only the `local menu` command and adds a marked floating-window/session-start block.
4. Reloads Hyprland and restores the backup if the compositor reports configuration errors.

Keep this project directory in place because the installed command links to it. Rofi and your original Rofi scripts remain installed.

To restore the previous launcher:

```sh
# Run from your checkout.
/usr/bin/python3 install.py --uninstall
```

Uninstall removes only this launcher's Hyprland block, restores the previous menu command, stops Arch Cast, and removes its executable link and desktop entry. It preserves later unrelated Hyprland edits, settings, and backups.

## Development and checks

```sh
QT_QPA_PLATFORM=offscreen /usr/bin/python3 -m unittest discover -s tests -v
/usr/bin/python3 -m compileall -q archcast
arch-cast doctor
hyprctl configerrors
```

`archcast/core.py` owns discovery, ranking, storage, and themes. `archcast/ui.py` owns the Qt window and interactions. `archcast/__main__.py` owns CLI and single-instance IPC.

This is an independent implementation based on oma-cast's public screenshots and documented behavior. No upstream source or Raycast assets were copied. Omarchy-specific functionality and Raycast extension compatibility are deliberately absent.
