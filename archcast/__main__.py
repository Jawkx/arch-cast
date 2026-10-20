"""CLI and single-instance local IPC."""
from __future__ import annotations

import argparse
import fcntl
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time

from .core import Catalog


def runtime_dir():
    base = Path(os.environ.get("XDG_RUNTIME_DIR", f"/tmp/arch-cast-{os.getuid()}"))
    base.mkdir(mode=0o700, parents=True, exist_ok=True)
    return base


def send(message):
    try:
        with socket.socket(socket.AF_UNIX) as client:
            client.settimeout(.35)
            client.connect(str(runtime_dir() / "arch-cast.sock"))
            client.sendall(json.dumps(message).encode() + b"\n")
            return client.recv(1024).decode()
    except (OSError, TimeoutError):
        return None


def doctor():
    from PyQt6.QtCore import QT_VERSION_STR
    checks = {
        "Qt": QT_VERSION_STR,
        "Desktop": os.environ.get("XDG_CURRENT_DESKTOP", "unknown"),
        "Display": os.environ.get("WAYLAND_DISPLAY", os.environ.get("DISPLAY", "missing")),
        "gtk-launch": shutil.which("gtk-launch") or "MISSING",
        "xdg-open": shutil.which("xdg-open") or "MISSING",
        "Resident": "running" if send({"command": "ping"}) else "not running, starts on first use",
        "Applications": len([entry for entry in Catalog().entries if entry.id.startswith("app:")]),
        "Config": str(Catalog().config_dir / "config.json"),
    }
    for key, value in checks.items():
        print(f"{key}: {value}")
    return 1 if "MISSING" in checks.values() or checks["Display"] == "missing" else 0


def main():
    parser = argparse.ArgumentParser(description="Arch Cast, a standalone native application launcher")
    parser.add_argument("command", nargs="?", default="toggle", choices=["toggle", "show", "hide", "quit", "doctor", "list", "run", "daemon"])
    parser.add_argument("argument", nargs="?", default="")
    args = parser.parse_args()
    if args.command == "doctor":
        return doctor()
    if args.command == "list":
        print(json.dumps([{"id": entry.id, "name": entry.name, "kind": entry.kind} for entry in Catalog().search(args.argument)], indent=2))
        return 0
    if args.command == "run" and not args.argument:
        parser.error("run requires an entry ID; use 'arch-cast list' to find one")
    message = {"command": args.command, "argument": args.argument}
    if send(message):
        return 0
    if args.command in ("hide", "quit"):
        return 0
    if args.command != "daemon":
        log_path = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "arch-cast/launcher.log"
        log_path.parent.mkdir(parents=True, exist_ok=True)
        with log_path.open("a") as log:
            process = subprocess.Popen([sys.executable, "-m", "archcast", "daemon"], cwd=Path(__file__).resolve().parent.parent,
                                       stdin=subprocess.DEVNULL, stdout=log, stderr=log, start_new_session=True, close_fds=True)
        for _ in range(50):
            time.sleep(.1)
            if send(message):
                return 0
            if process.poll() is not None:
                break
        print(f"arch-cast: could not start; see {log_path}", file=sys.stderr)
        return 1
    # A lock prevents simultaneous cold-start shortcuts from creating two windows.
    lock = open(runtime_dir() / "arch-cast.lock", "w")
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        for _ in range(30):
            time.sleep(.1)
            if send(message):
                return 0
        print("arch-cast: resident did not respond", file=sys.stderr)
        return 1
    # Publish the Qt accessibility tree even when no screen reader is running.
    os.environ.setdefault("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "1")
    from PyQt6.QtCore import QTimer
    from PyQt6.QtNetwork import QLocalServer
    from PyQt6.QtGui import QIcon
    from PyQt6.QtWidgets import QApplication, QMessageBox
    from .ui import Launcher
    QApplication.setDesktopFileName("arch-cast")
    app = QApplication(["arch-cast"])
    app.setApplicationName("arch-cast")
    app.setQuitOnLastWindowClosed(False)
    QIcon.setThemeSearchPaths(QIcon.themeSearchPaths() + [str(Path.home() / ".local/share/icons"), str(Path.home() / ".icons"), "/usr/local/share/icons", "/usr/share/icons"])
    if not QIcon.themeName():
        QIcon.setThemeName("breeze-dark")
    QIcon.setFallbackThemeName("breeze-dark")
    catalog = Catalog()
    launcher = Launcher(catalog)
    server = QLocalServer()
    server.setSocketOptions(QLocalServer.SocketOption.UserAccessOption)
    address = str(runtime_dir() / "arch-cast.sock")
    QLocalServer.removeServer(address)
    if not server.listen(address):
        print(server.errorString(), file=sys.stderr)
        return 1
    connections = set()

    def dispatch(message):
        command = message.get("command")
        if command == "toggle":
            if launcher.isVisible():
                launcher.hide()
            else:
                launcher.present()
        elif command == "show":
            launcher.present()
        elif command == "hide":
            launcher.hide()
        elif command == "quit":
            app.quit()
        elif command == "run":
            entry = next((entry for entry in catalog.entries if entry.id == message.get("argument")), None)
            if entry:
                launcher.run_entry(entry)
            else:
                launcher.present()
                QMessageBox.warning(launcher, "Unknown command", "No entry with ID " + message.get("argument", ""))

    def accept():
        while server.hasPendingConnections():
            connection = server.nextPendingConnection()
            connections.add(connection)
            buffer = bytearray()
            def receive(connection=connection, buffer=buffer):
                buffer.extend(bytes(connection.readAll()))
                if len(buffer) > 16384:
                    connection.abort()
                    return
                if b"\n" in buffer:
                    try:
                        message = json.loads(buffer.split(b"\n", 1)[0])
                        if not isinstance(message, dict):
                            raise ValueError("Expected an object")
                        connection.write(b"ok\n")
                        connection.flush()
                        connection.disconnectFromServer()
                        QTimer.singleShot(0, lambda: dispatch(message))
                    except (ValueError, UnicodeError):
                        connection.abort()
            connection.readyRead.connect(receive)
            def cleanup(connection=connection):
                connections.discard(connection)
                connection.deleteLater()
            connection.disconnected.connect(cleanup)
            if connection.bytesAvailable():
                receive()
    server.newConnection.connect(accept)
    if args.command != "daemon":
        QTimer.singleShot(0, lambda: dispatch(message))
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
