"""Application discovery, persisted preferences, and ranked search. No UI dependencies."""
from __future__ import annotations

import ast
import configparser
import json
import math
import operator
import os
from pathlib import Path
import re
import shutil
import tempfile
import time
from dataclasses import dataclass
from urllib.parse import quote_plus

MOCHA = {
    "background": "#1e1e2e", "surface": "#181825", "selection": "#313244",
    "border": "#45475a", "text": "#cdd6f4", "muted": "#a6adc8",
    "accent": "#cba6f7", "success": "#a6e3a1",
}
THEMES = {
    "Catppuccin Mocha": MOCHA,
    "Catppuccin Macchiato": dict(zip(MOCHA, ["#24273a", "#1e2030", "#363a4f", "#494d64", "#cad3f5", "#a5adcb", "#c6a0f6", "#a6da95"])),
    "Catppuccin Latte": dict(zip(MOCHA, ["#eff1f5", "#e6e9ef", "#ccd0da", "#bcc0cc", "#4c4f69", "#6c6f85", "#8839ef", "#40a02b"])),
    "Tokyo Night": dict(zip(MOCHA, ["#1a1b26", "#16161e", "#292e42", "#414868", "#c0caf5", "#9aa5ce", "#7aa2f7", "#9ece6a"])),
}
DEFAULT_FAVORITES = ["app:google-chrome.desktop", "app:com.mitchellh.ghostty.desktop", "app:steam.desktop", "app:chrome-hnpfjngllnobngcgfapefoaidbinmjnm-Default.desktop"]


def read_json(path: Path, default):
    try:
        value = json.loads(path.read_text())
        return value if isinstance(value, type(default)) else default
    except (OSError, ValueError):
        return default


def write_json(path: Path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=path.parent, prefix=".arch-cast-")
    try:
        with os.fdopen(fd, "w") as stream:
            json.dump(value, stream, indent=2, ensure_ascii=False)
            stream.write("\n")
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


@dataclass(frozen=True)
class Entry:
    id: str
    name: str
    kind: str = "Application"
    detail: str = ""
    icon: str = "application-x-executable"
    target: str = ""
    keywords: str = ""


def discover_apps(directories=None) -> list[Entry]:
    if directories is None:
        directories = [Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share")) / "applications"]
        directories += [Path(p) / "applications" for p in os.environ.get("XDG_DATA_DIRS", "/usr/local/share:/usr/share").split(":") if p]
    entries = {}
    seen = set()
    desktop = set(os.environ.get("XDG_CURRENT_DESKTOP", "").split(":"))
    for directory in directories:
        directory = Path(directory)
        if not directory.is_dir():
            continue
        for path in sorted(directory.rglob("*.desktop")):
            desktop_id = str(path.relative_to(directory)).replace("/", "-")
            if desktop_id in seen:
                continue
            seen.add(desktop_id)
            parser = configparser.ConfigParser(interpolation=None, strict=False)
            try:
                parser.read(path, encoding="utf-8")
                item = parser["Desktop Entry"]
                if item.get("Type", "Application") != "Application":
                    continue
                if item.getboolean("Hidden", fallback=False) or item.getboolean("NoDisplay", fallback=False):
                    continue
                if item.get("OnlyShowIn") and not desktop.intersection(item["OnlyShowIn"].strip(";").split(";")):
                    continue
                if desktop.intersection(item.get("NotShowIn", "").strip(";").split(";")):
                    continue
                if item.get("TryExec") and not shutil.which(item["TryExec"]):
                    continue
                if not item.get("Exec") and not item.getboolean("DBusActivatable", fallback=False):
                    continue
                name = item.get("Name", path.stem)
                entries[desktop_id] = Entry("app:" + desktop_id, name, detail=item.get("GenericName", ""), icon=item.get("Icon", "application-x-executable"), target=str(path), keywords=" ".join([item.get("Keywords", ""), item.get("Comment", ""), item.get("Exec", ""), desktop_id]))
            except (OSError, ValueError, configparser.Error):
                continue
    return sorted(entries.values(), key=lambda entry: entry.name.casefold())


def calculate(query: str) -> str | None:
    expression = query.strip().removeprefix("=").strip().replace("×", "*").replace("÷", "/").replace("^", "**")
    if len(expression) > 160 or not re.search(r"\d", expression) or not re.search(r"[+*/%()^-]", expression):
        return None
    binary = {ast.Add: operator.add, ast.Sub: operator.sub, ast.Mult: operator.mul, ast.Div: operator.truediv, ast.FloorDiv: operator.floordiv, ast.Mod: operator.mod, ast.Pow: operator.pow}
    def evaluate(node):
        if isinstance(node, ast.Constant) and type(node.value) in (int, float):
            value = node.value
        elif isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.UAdd, ast.USub)):
            value = evaluate(node.operand) * (-1 if isinstance(node.op, ast.USub) else 1)
        elif isinstance(node, ast.BinOp) and type(node.op) in binary:
            left, right = evaluate(node.left), evaluate(node.right)
            if isinstance(node.op, ast.Pow) and (abs(right) > 100 or abs(left) > 1e12):
                raise ValueError("Exponent too large")
            value = binary[type(node.op)](left, right)
        else:
            raise ValueError("Not arithmetic")
        if not isinstance(value, (int, float)) or abs(value) > 1e100 or not math.isfinite(value):
            raise ValueError("Result too large")
        return value
    try:
        value = evaluate(ast.parse(expression, mode="eval").body)
        return format(value, ".12g")
    except (ValueError, SyntaxError, ZeroDivisionError, OverflowError, TypeError, RecursionError):
        return None


def match_score(query: str, entry: Entry, alias: str = "") -> float:
    query, name, alias = query.casefold(), entry.name.casefold(), alias.casefold()
    if alias and query == alias:
        return 1500
    if query == name:
        return 1200
    if name.startswith(query):
        return 900 - len(name) * .1
    words = re.findall(r"[\w]+", name)
    if "".join(word[0] for word in words).startswith(query):
        return 780
    if query in name:
        return 650 - name.index(query)
    haystack = " ".join([name, alias, entry.keywords.casefold(), entry.detail.casefold()])
    tokens = query.split()
    if tokens and all(token in haystack for token in tokens):
        return 400
    # Subsequence matches tolerate abbreviations without matching every long description.
    position, gaps = -1, 0
    for char in query:
        next_position = name.find(char, position + 1)
        if next_position < 0:
            return 0
        gaps += next_position - position - 1
        position = next_position
    return max(20, 250 - gaps * 8) if query else 0


class Catalog:
    def __init__(self, config_dir=None, state_dir=None, applications=None, clock=time.time):
        self.config_dir = Path(config_dir or Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "arch-cast")
        self.state_dir = Path(state_dir or Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "arch-cast")
        self.clock = clock
        self._applications = applications
        self.reload()

    def reload(self):
        self.config = read_json(self.config_dir / "config.json", {})
        self.state = read_json(self.state_dir / "state.json", {})
        for key, default in {"favorites": DEFAULT_FAVORITES.copy(), "hidden": [], "aliases": {}, "usage": {}, "learned": {}, "history": []}.items():
            if not isinstance(self.state.get(key), type(default)):
                self.state[key] = default
        self.entries = list(self._applications if self._applications is not None else discover_apps())
        self.entries += [
            Entry("settings", "Launcher Settings", "Command", "Theme, appearance & hidden results", "preferences-desktop-theme"),
            Entry("reload", "Refresh Applications", "Command", "Rescan installed apps and custom commands", "view-refresh"),
            Entry("config", "Open Launcher Config", "Command", "Edit custom commands and theme colors", "text-x-generic"),
        ]
        commands = self.config.get("commands", [])
        if isinstance(commands, list):
            for command in commands:
                if isinstance(command, dict) and all(isinstance(command.get(key), str) and command[key] for key in ("id", "name", "command")):
                    self.entries.append(Entry("custom:" + command["id"], command["name"], "Command", command.get("description", "Custom command"), command.get("icon", "utilities-terminal"), command["command"], command.get("keywords", "")))

    def theme(self):
        colors = THEMES.get(self.config.get("theme"), MOCHA).copy()
        custom = self.config.get("colors", {})
        if isinstance(custom, dict):
            for key, value in custom.items():
                if key in colors and isinstance(value, str) and re.fullmatch(r"#[0-9a-fA-F]{6}", value):
                    colors[key] = value
        return colors

    def save_config(self):
        write_json(self.config_dir / "config.json", self.config)

    def save_state(self):
        write_json(self.state_dir / "state.json", self.state)

    def search(self, query: str, limit=60) -> list[Entry]:
        query = query.strip()
        ranked = []
        favorites = self.state["favorites"]
        for entry in self.entries:
            if entry.id in self.state["hidden"]:
                continue
            score = match_score(query, entry, self.state["aliases"].get(entry.id, "")) if query else 1
            learned = bool(query) and self.state["learned"].get(query.casefold()) == entry.id
            if query and not score and not learned:
                continue
            usage = self.state["usage"].get(entry.id, {})
            if isinstance(usage, dict):
                age_days = max(0, (self.clock() - usage.get("last", 0)) / 86400)
                score += min(80, math.log2(usage.get("count", 0) + 1) * 12) / (1 + age_days / 7)
            if entry.id in favorites:
                score += 35 if query else 500 - favorites.index(entry.id)
            if learned:
                score += 1000
            if not query and entry.id == "settings":
                score += 20
            ranked.append((score, entry))
        ranked.sort(key=lambda item: (-item[0], item[1].name.casefold()))
        result = [entry for _, entry in ranked[:limit]]
        if query:
            answer = calculate(query)
            if answer is not None:
                result.insert(0, Entry("calculator", answer, "Calculator", query + "  ·  Enter to copy", "accessories-calculator", answer))
            result.extend([
                Entry("web:google", "Search Google", "Web Search", query, "internet-web-browser", "https://www.google.com/search?q=" + quote_plus(query)),
                Entry("web:duckduckgo", "Search DuckDuckGo", "Web Search", query, "system-search", "https://duckduckgo.com/?q=" + quote_plus(query)),
            ])
        return result

    def record(self, entry, query):
        if entry.id not in {"calculator", "settings", "reload", "config"} and not entry.id.startswith("web:"):
            previous = self.state["usage"].get(entry.id, {})
            self.state["usage"][entry.id] = {"count": previous.get("count", 0) + 1, "last": self.clock()}
            if query.strip():
                self.state["learned"][query.strip().casefold()] = entry.id
                self.state["learned"] = dict(list(self.state["learned"].items())[-500:])
        if query.strip():
            self.state["history"] = ([query] + [q for q in self.state["history"] if q != query])[:100]
        self.save_state()

    def favorite(self, entry):
        favorites = self.state["favorites"]
        if entry.id in favorites:
            favorites.remove(entry.id)
        else:
            favorites.append(entry.id)
        self.save_state()

    def alias(self, entry, alias):
        if alias.strip():
            self.state["aliases"][entry.id] = alias.strip()
        else:
            self.state["aliases"].pop(entry.id, None)
        self.save_state()

    def hide(self, entry):
        if entry.id not in self.state["hidden"]:
            self.state["hidden"].append(entry.id)
        self.save_state()

    def reset_ranking(self, entry):
        self.state["usage"].pop(entry.id, None)
        self.state["learned"] = {key: value for key, value in self.state["learned"].items() if value != entry.id}
        self.save_state()
