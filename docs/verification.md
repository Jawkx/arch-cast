# Verification

The Rust port is tested on the actual Hyprland 0.56.2 Wayland desktop. Screenshots in this directory show the GPUI application, not HTML mockups.

## Automated checks

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

All 26 tests pass. Formatting, Clippy with warnings denied, and the locked release build also pass.

The suite includes:

- Arithmetic precedence, right-associative powers, signed division/remainders, and rejection of code, non-finite results, and expensive expressions.
- Name, acronym, keyword, abbreviation, Unicode, learned-query, and web-fallback search.
- Favorites, aliases, hidden entries, persistence, bounded history, corrupt files, typed validation, and preservation of unknown config keys.
- XDG desktop-entry precedence, hidden overrides, desktop filters, nested IDs, and localized names.
- Socket round trips, single-instance locking, socket permissions, and rejection of malformed requests.
- Headless GPUI interaction tests using real text input and key dispatch for search, action filtering, favorites, aliases, section navigation, Escape handling, clipboard arithmetic, settings preview/cancel/save, invalid settings, and restoring hidden results.
- Installer integration tests in temporary home directories, with a fake compositor command. These exercise installation, reinstallation, uninstall, preservation of later user edits, and rollback of the executable symlink, config, manifest, and desktop entry after compositor rejection.

The GPUI tests do not need a display server or GPU. They use GPUI Kit's test platform. No Python or Qt test runner is needed.

## Desktop checks

Use a separate config/state directory for exploratory UI checks so test favorites, aliases, and appearance changes do not affect the user's saved launcher state. Keep the real `XDG_RUNTIME_DIR` for access to the Wayland compositor. Stop any existing resident before starting an isolated one, since the socket deliberately remains single-instance.

Checked during the port:

- A native, non-XWayland `arch-cast` window opens at 840×584 logical pixels on a 1.5× display.
- The search field receives keyboard focus and renders system application icons.
- Real key events from a temporary uinput keyboard search for Ghostty and open the Ctrl+K action panel.
- The action panel saves a `gt` alias for Ghostty, finds it through search, and removes the test alias.
- Enter on `12 * (3 + 4)` writes `84` to the real Wayland clipboard without closing the window.
- An unmatched query leaves only Google and DuckDuckGo fallback results.
- A saved Latte configuration reloads when a new window opens.
- The settings view renders the preset controls, hex inputs, color swatches, font controls, and save/cancel actions.
- Closing the window leaves the resident process running.
- Five successive `show` commands create exactly one launcher window.
- Focusing the existing terminal dismisses the launcher.
- `toggle` reopens the window under the same resident PID.
- The new CLI can stop the old Python resident through the existing socket protocol.
- Launching Ghostty through GIO opens a native app window. That window survives quitting the launcher. Only the newly launched test window was closed afterward.
- An explicit custom command completes after the resident exits.
- The installed release executable is a stripped native ELF binary. Super+Space toggles it without restarting its daemon.
- `arch-cast doctor` passes and `hyprctl configerrors` is empty after installation.
- SHA-256 checks confirm the user's config, state, and original install manifest are byte-for-byte unchanged by the upgrade and final keyboard checks.

Virtual-keyboard protocol injection through `wtype` was unreliable with this GPUI/Hyprland combination. The desktop keyboard checks use uinput instead. The temporary input devices are destroyed after each invocation.

## Manual regression checklist

1. Toggle with Super+Space. Type an app name, use arrows, and launch it. Confirm closing the launcher does not close the app.
2. Search for `12 * (3 + 4)`, press Enter, and read `84` from the clipboard. The launcher should remain open.
3. Open Ctrl+K, filter to Set Alias, save an alias, and search using it. Remove the test alias afterward.
4. Toggle a favorite, hide a result, and restore hidden results in settings.
5. Preview Latte, cancel, and confirm the saved theme returns. Save a theme, reopen the launcher, and confirm it persists.
6. Enter an invalid color or width. Saving should fail without overwriting the saved config.
7. Search for an unmatched query. Only the two web fallback results should remain.
8. Run `arch-cast doctor` and `hyprctl configerrors` after installation.

Installation backups and the original Rofi menu command live under the user's Arch Cast config directory, outside this repository. The original install manifest is retained when upgrading from the Python version.

## Captures

- [Default Mocha](screenshots/mocha.png)
- [Application search](screenshots/search.png)
- [Action panel](screenshots/actions.png)
- [Settings](screenshots/settings.png)
- [Saved Latte theme](screenshots/latte.png)
- [Calculator](screenshots/calculator.png)
- [Web fallbacks](screenshots/web-fallbacks.png)
