"""Native keyboard-first launcher and theme editor."""
from __future__ import annotations

import copy
from pathlib import Path
import shlex
import subprocess

from PyQt6.QtCore import Qt, QEvent, QSize, QTimer, QRectF
from PyQt6.QtGui import QColor, QFont, QIcon, QPainter, QPen, QCursor
from PyQt6.QtWidgets import (
    QApplication, QWidget, QVBoxLayout, QHBoxLayout, QLabel, QLineEdit,
    QListWidget, QListWidgetItem, QStyledItemDelegate, QStyle, QFrame,
    QPushButton, QDialog, QFormLayout, QComboBox, QSpinBox, QDialogButtonBox,
    QColorDialog, QInputDialog, QMessageBox, QAbstractItemView,
)

from .core import Catalog, Entry, THEMES


def detached(argv):
    return subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL, close_fds=True, start_new_session=True)


def icon_for(name):
    if Path(name).is_absolute():
        return QIcon(name)
    icon = QIcon.fromTheme(name)
    return icon if not icon.isNull() else QApplication.style().standardIcon(QStyle.StandardPixmap.SP_ComputerIcon)


class ResultDelegate(QStyledItemDelegate):
    def __init__(self, launcher):
        super().__init__(launcher)
        self.launcher = launcher

    def paint(self, painter, option, index):
        painter.save()
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        colors = self.launcher.catalog.theme()
        rect = option.rect.adjusted(4, 2, -4, -2)
        entry = index.data(Qt.ItemDataRole.UserRole)
        if not entry:
            painter.setPen(QColor(colors["muted"]))
            font = QFont(self.launcher.font())
            font.setPointSizeF(max(9, font.pointSizeF() - 1))
            font.setWeight(QFont.Weight.DemiBold)
            painter.setFont(font)
            painter.drawText(rect.adjusted(14, 3, 0, 0), Qt.AlignmentFlag.AlignVCenter, index.data())
            painter.restore()
            return
        if option.state & QStyle.StateFlag.State_Selected:
            painter.setBrush(QColor(colors["selection"]))
            painter.setPen(Qt.PenStyle.NoPen)
            painter.drawRoundedRect(QRectF(rect), 9, 9)
            painter.setPen(QPen(QColor(colors["accent"]), 2))
            painter.drawLine(rect.left() + 1, rect.top() + 12, rect.left() + 1, rect.bottom() - 12)
        icon_for(entry.icon).paint(painter, rect.left() + 16, rect.center().y() - 13, 26, 26)
        painter.setFont(self.launcher.font())
        painter.setPen(QColor(colors["text"]))
        metrics = painter.fontMetrics()
        kind_width = metrics.horizontalAdvance(entry.kind) + 22
        text_left = rect.left() + 58
        available = rect.width() - 70 - kind_width
        title = metrics.elidedText(entry.name, Qt.TextElideMode.ElideRight, max(40, available))
        painter.drawText(text_left, rect.top(), available, rect.height(), Qt.AlignmentFlag.AlignVCenter, title)
        title_width = metrics.horizontalAdvance(title)
        alias = self.launcher.catalog.state["aliases"].get(entry.id, "")
        detail = alias or entry.detail
        if entry.id in self.launcher.catalog.state["favorites"]:
            detail = "★" + ("  " + detail if detail else "")
        remaining = available - title_width - 16
        painter.setPen(QColor(colors["muted"]))
        if remaining > 35:
            detail_font = QFont(self.launcher.font())
            detail_font.setPointSizeF(max(9, detail_font.pointSizeF() - 1))
            painter.setFont(detail_font)
            detail = painter.fontMetrics().elidedText(detail, Qt.TextElideMode.ElideRight, remaining)
            painter.drawText(text_left + title_width + 13, rect.top(), remaining, rect.height(), Qt.AlignmentFlag.AlignVCenter, detail)
        small = QFont(self.launcher.font())
        small.setPointSizeF(max(9, small.pointSizeF() - 1))
        painter.setFont(small)
        painter.drawText(rect.right() - kind_width, rect.top(), kind_width - 14, rect.height(), Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter, entry.kind)
        painter.restore()

    def sizeHint(self, option, index):
        return QSize(300, 52 if index.data(Qt.ItemDataRole.UserRole) else 32)


class SettingsDialog(QDialog):
    def __init__(self, launcher):
        super().__init__(launcher)
        self.launcher = launcher
        self.catalog = launcher.catalog
        self.original = copy.deepcopy(self.catalog.config)
        self.setWindowTitle("Arch Cast Settings")
        self.setMinimumWidth(580)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(26, 22, 26, 22)
        heading = QLabel("Make it yours")
        heading.setObjectName("dialogTitle")
        layout.addWidget(heading)
        subtitle = QLabel("Themes belong to this launcher, not your desktop.")
        subtitle.setObjectName("muted")
        layout.addWidget(subtitle)
        form = QFormLayout()
        form.setVerticalSpacing(10)
        self.preset = QComboBox()
        self.preset.addItems(THEMES)
        self.preset.setCurrentText(self.catalog.config.get("theme", "Catppuccin Mocha"))
        form.addRow("Theme preset", self.preset)
        self.colors = {}
        for key, value in self.catalog.theme().items():
            row = QHBoxLayout()
            field = QLineEdit(value)
            field.setAccessibleName(key + " color")
            field.setMaxLength(7)
            field.setFixedWidth(140)
            button = QPushButton("Choose…")
            button.clicked.connect(lambda checked=False, field=field: self.pick_color(field))
            row.addWidget(field)
            row.addWidget(button)
            row.addStretch()
            form.addRow(key.capitalize(), row)
            self.colors[key] = field
        self.font_size = QSpinBox()
        self.font_size.setRange(10, 20)
        self.font_size.setValue(launcher.font_size())
        form.addRow("Font size", self.font_size)
        self.width = QSpinBox()
        self.width.setRange(640, 1200)
        self.width.setSingleStep(20)
        self.width.setValue(launcher.window_width())
        form.addRow("Window width", self.width)
        layout.addLayout(form)
        self.preset.currentTextChanged.connect(self.select_preset)
        restore = QPushButton(f"Restore hidden results ({len(self.catalog.state['hidden'])})")
        restore.clicked.connect(lambda: self.restore_hidden(restore))
        layout.addWidget(restore)
        location = QLabel("Custom commands: ~/.config/arch-cast/config.json")
        location.setObjectName("muted")
        layout.addWidget(location)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Save | QDialogButtonBox.StandardButton.Cancel | QDialogButtonBox.StandardButton.Apply)
        buttons.accepted.connect(self.save)
        buttons.rejected.connect(self.reject)
        buttons.button(QDialogButtonBox.StandardButton.Apply).setText("Preview")
        buttons.button(QDialogButtonBox.StandardButton.Apply).clicked.connect(self.preview)
        layout.addWidget(buttons)
        self.setStyleSheet(launcher.styleSheet())

    def pick_color(self, field):
        value = QColorDialog.getColor(QColor(field.text()), self, "Choose theme color")
        if value.isValid():
            field.setText(value.name())

    def select_preset(self, name):
        for key, value in THEMES[name].items():
            self.colors[key].setText(value)
        self.preview()

    def restore_hidden(self, button):
        self.catalog.state["hidden"] = []
        self.catalog.save_state()
        button.setText("All hidden results restored")
        self.launcher.refresh()

    def preview(self):
        import re
        if any(not re.fullmatch(r"#[0-9a-fA-F]{6}", field.text()) for field in self.colors.values()):
            QMessageBox.warning(self, "Invalid color", "Use six-digit hex colors, such as #cba6f7.")
            return False
        self.catalog.config.update(theme=self.preset.currentText(), colors={key: field.text() for key, field in self.colors.items()}, font_size=self.font_size.value(), width=self.width.value())
        self.launcher.apply_theme()
        self.setStyleSheet(self.launcher.styleSheet())
        return True

    def save(self):
        if self.preview():
            self.catalog.save_config()
            self.accept()

    def reject(self):
        self.catalog.config = self.original
        self.launcher.apply_theme()
        super().reject()


class Launcher(QWidget):
    def __init__(self, catalog: Catalog):
        super().__init__()
        self.catalog = catalog
        self.setWindowTitle("Arch Cast")
        self.setObjectName("launcher")
        self.setWindowFlags(Qt.WindowType.Window | Qt.WindowType.FramelessWindowHint)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.dialog_open = False
        self.history_index = -1
        self._opening = False
        self.action_entry = None
        self.setAccessibleName("Arch Cast application launcher")
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(0)
        self.shell = QFrame()
        self.shell.setObjectName("shell")
        outer.addWidget(self.shell)
        layout = QVBoxLayout(self.shell)
        layout.setContentsMargins(1, 1, 1, 1)
        layout.setSpacing(0)
        header = QFrame()
        header.setObjectName("header")
        header.setFixedHeight(82)
        row = QHBoxLayout(header)
        row.setContentsMargins(24, 0, 22, 0)
        mark = QLabel("◈")
        mark.setObjectName("mark")
        row.addWidget(mark)
        self.search = QLineEdit()
        self.search.setObjectName("search")
        self.search.setPlaceholderText("Search for apps and commands…")
        self.search.setAccessibleName("Search apps and commands")
        self.search.setClearButtonEnabled(True)
        self.search.textChanged.connect(self.refresh)
        row.addWidget(self.search, 1)
        esc = QLabel("esc")
        esc.setObjectName("keycap")
        esc.setFixedHeight(25)
        row.addWidget(esc)
        layout.addWidget(header)
        self.results = QListWidget()
        self.results.setAccessibleName("Search results")
        self.results.setObjectName("results")
        self.results.setItemDelegate(ResultDelegate(self))
        self.results.setVerticalScrollMode(QAbstractItemView.ScrollMode.ScrollPerPixel)
        self.results.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.results.setFocusPolicy(Qt.FocusPolicy.NoFocus)
        self.results.setMouseTracking(True)
        self.results.itemClicked.connect(lambda item: self.run_entry(item.data(Qt.ItemDataRole.UserRole)))
        self.results.currentItemChanged.connect(self.update_footer)
        layout.addWidget(self.results, 1)
        self.footer = QFrame()
        self.footer.setObjectName("footer")
        self.footer.setFixedHeight(48)
        row = QHBoxLayout(self.footer)
        row.setContentsMargins(18, 0, 15, 0)
        self.brand = QLabel("◈  arch cast")
        self.brand.setObjectName("muted")
        row.addWidget(self.brand)
        settings = QPushButton("Settings")
        settings.setObjectName("quiet")
        settings.clicked.connect(self.settings)
        row.addWidget(settings)
        row.addStretch()
        self.run_hint = QLabel("Open  ↵")
        row.addWidget(self.run_hint)
        actions = QPushButton("Actions  Ctrl K")
        actions.setObjectName("quiet")
        actions.clicked.connect(self.toggle_actions)
        row.addWidget(actions)
        layout.addWidget(self.footer)
        self.panel = QFrame(self.shell)
        self.panel.setObjectName("actionPanel")
        panel_layout = QVBoxLayout(self.panel)
        panel_layout.setContentsMargins(8, 8, 8, 8)
        self.action_title = QLabel("Actions")
        self.action_title.setObjectName("muted")
        panel_layout.addWidget(self.action_title)
        self.actions = QListWidget()
        self.actions.setAccessibleName("Actions for selected result")
        self.actions.setObjectName("actions")
        self.actions.itemClicked.connect(self.execute_action)
        panel_layout.addWidget(self.actions)
        self.action_search = QLineEdit()
        self.action_search.setPlaceholderText("Search for actions…")
        self.action_search.setAccessibleName("Search actions")
        self.action_search.textChanged.connect(self.filter_actions)
        panel_layout.addWidget(self.action_search)
        self.panel.hide()
        self.apply_theme()
        QApplication.instance().installEventFilter(self)
        self.refresh()
        self.search.setFocus()

    def font_size(self):
        value = self.catalog.config.get("font_size", 12)
        return max(10, min(20, value)) if type(value) is int else 12

    def window_width(self):
        value = self.catalog.config.get("width", 840)
        return max(640, min(1200, value)) if type(value) is int else 840

    def apply_theme(self):
        c = self.catalog.theme()
        font = QFont(self.catalog.config.get("font", "Inter"), self.font_size())
        self.setFont(font)
        self.setFixedSize(self.window_width(), 584)
        self.setStyleSheet(f"""
            QWidget {{ color: {c['text']}; font-family: '{font.family()}'; font-size: {self.font_size()}pt; }}
            QFrame#shell {{ background: {c['background']}; border: 1px solid {c['border']}; border-radius: 14px; }}
            QFrame#header {{ border-bottom: 1px solid {c['border']}; }}
            QLabel {{ background: transparent; border: none; }}
            QLabel#mark {{ color: {c['accent']}; font-size: 30px; padding-right: 10px; }}
            QLabel#muted {{ color: {c['muted']}; font-size: 10pt; }}
            QLabel#keycap {{ color: {c['muted']}; background: {c['selection']}; border-radius: 5px; padding: 3px 7px; font-size: 9pt; }}
            QLabel#dialogTitle {{ font-size: 20pt; font-weight: 600; }}
            QLineEdit {{ background: {c['surface']}; border: 1px solid {c['border']}; border-radius: 6px; padding: 8px; selection-background-color: {c['accent']}; selection-color: {c['background']}; }}
            QLineEdit:focus {{ border-color: {c['accent']}; }}
            QLineEdit#search {{ background: transparent; border: none; padding: 0; font-size: 19px; }}
            QListWidget {{ background: transparent; border: none; outline: none; padding: 8px; }}
            QListWidget#actions {{ padding: 0; }}
            QListWidget#actions::item {{ padding: 10px 8px; border-radius: 6px; }}
            QListWidget#actions::item:selected {{ background: {c['selection']}; color: {c['text']}; }}
            QFrame#footer {{ background: {c['surface']}; border-top: 1px solid {c['border']}; border-bottom-left-radius: 14px; border-bottom-right-radius: 14px; }}
            QFrame#actionPanel {{ background: {c['surface']}; border: 1px solid {c['border']}; border-radius: 10px; }}
            QPushButton {{ background: {c['selection']}; border: 1px solid {c['border']}; border-radius: 6px; padding: 7px 12px; }}
            QPushButton:hover {{ border-color: {c['accent']}; color: {c['accent']}; }}
            QPushButton#quiet {{ background: transparent; border: none; color: {c['muted']}; font-size: 10pt; padding: 5px 8px; }}
            QPushButton#quiet:hover {{ color: {c['accent']}; background: {c['selection']}; }}
            QScrollBar:vertical {{ background: transparent; width: 5px; margin: 5px 0; }}
            QScrollBar::handle:vertical {{ background: {c['border']}; border-radius: 2px; min-height: 25px; }}
            QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {{ height: 0; }}
            QScrollBar::add-page:vertical, QScrollBar::sub-page:vertical {{ background: none; }}
            QDialog, QMessageBox {{ background: {c['background']}; }}
            QComboBox, QSpinBox {{ background: {c['surface']}; border: 1px solid {c['border']}; border-radius: 5px; padding: 6px; }}
            QComboBox QAbstractItemView {{ background: {c['surface']}; selection-background-color: {c['selection']}; }}
        """)
        self.results.viewport().update()

    def refresh(self, *_):
        self.close_actions()
        self.results.clear()
        query = self.search.text()
        entries = self.catalog.search(query)
        previous_section = None
        first = None
        for entry in entries:
            if entry.kind == "Web Search":
                section = "Search the web"
            elif query.strip():
                section = "Calculator" if entry.kind == "Calculator" else "Results"
            elif entry.id in self.catalog.state["favorites"]:
                section = "Favorites"
            else:
                section = "Suggestions"
            if section != previous_section:
                label = QListWidgetItem(section)
                label.setFlags(Qt.ItemFlag.NoItemFlags)
                self.results.addItem(label)
                previous_section = section
            item = QListWidgetItem(entry.name)
            item.setData(Qt.ItemDataRole.UserRole, entry)
            item.setToolTip(entry.name + ("\n" + entry.detail if entry.detail else ""))
            item.setData(Qt.ItemDataRole.AccessibleTextRole, f"{entry.name}, {entry.kind}")
            self.results.addItem(item)
            if first is None:
                first = item
        if first:
            self.results.setCurrentItem(first)
            self.results.scrollToTop()
        self.update_footer()

    def selected(self):
        item = self.results.currentItem()
        return item.data(Qt.ItemDataRole.UserRole) if item else None

    def update_footer(self, *_):
        entry = self.selected()
        self.run_hint.setText(("Copy" if entry and entry.id == "calculator" else "Open") + "  ↵")

    def navigate(self, delta, sections=False):
        row = self.results.currentRow()
        candidate = row + delta
        while 0 <= candidate < self.results.count():
            item = self.results.item(candidate)
            if item.data(Qt.ItemDataRole.UserRole):
                if not sections or not self.results.item(candidate - delta).data(Qt.ItemDataRole.UserRole):
                    self.results.setCurrentRow(candidate)
                    return
            candidate += delta

    def run_entry(self, entry):
        if not entry:
            return
        self.close_actions()
        try:
            if entry.id == "settings":
                self.settings()
                return
            if entry.id == "reload":
                self.catalog.reload()
                self.apply_theme()
                self.refresh()
                self.toast("Applications refreshed")
                return
            if entry.id == "calculator":
                QApplication.clipboard().setText(entry.target)
                self.toast("Copied " + entry.target)
                return
            if entry.id == "config":
                if not (self.catalog.config_dir / "config.json").exists():
                    self.catalog.save_config()
                detached(["xdg-open", str(self.catalog.config_dir / "config.json")])
            elif entry.id.startswith("app:"):
                # GIO handles desktop field codes, quoting, Terminal and DBus activation.
                # Launch by ID so user desktop overrides retain their normal semantics.
                detached(["gtk-launch", entry.id.removeprefix("app:")])
            elif entry.id.startswith("web:"):
                detached(["xdg-open", entry.target])
            elif entry.id.startswith("custom:"):
                # Shell commands are explicitly user-authored config, never search input.
                detached(["/bin/sh", "-c", entry.target])
            self.catalog.record(entry, self.search.text())
            self.hide()
        except OSError as error:
            QMessageBox.warning(self, "Could not launch", str(error))

    def settings(self):
        self.close_actions()
        self.dialog_open = True
        try:
            SettingsDialog(self).exec()
        finally:
            self.dialog_open = False
        self.refresh()
        self.search.setFocus()

    def toast(self, message):
        self.brand.setText(message)
        QTimer.singleShot(2200, lambda: self.brand.setText("◈  arch cast"))

    def toggle_actions(self):
        if self.panel.isVisible():
            self.close_actions()
            return
        entry = self.selected()
        if not entry:
            return
        self.action_entry = entry
        self.action_title.setText(entry.name)
        self.action_search.clear()
        self.action_specs = [("Open" if entry.id != "calculator" else "Copy result", "↵", lambda: self.run_entry(entry))]
        if entry.id.startswith(("app:", "custom:")):
            favorite = entry.id in self.catalog.state["favorites"]
            self.action_specs += [
                ("Remove from Favorites" if favorite else "Add to Favorites", "Ctrl Shift F", lambda: self.mutate(lambda: self.catalog.favorite(entry))),
                ("Set Alias", "", lambda: self.set_alias(entry)),
                ("Copy Desktop ID" if entry.id.startswith("app:") else "Copy Command", "Ctrl Shift C", lambda: self.copy_entry(entry)),
                ("Reset Ranking", "", lambda: self.mutate(lambda: self.catalog.reset_ranking(entry))),
                ("Hide Result", "Alt Shift D", lambda: self.mutate(lambda: self.catalog.hide(entry))),
            ]
        self.action_specs.append(("Launcher Settings", "Ctrl ,", self.settings))
        self.filter_actions()
        self.panel.setGeometry(self.width() - 394, self.height() - 458, 376, 396)
        self.panel.show()
        self.panel.raise_()
        self.action_search.setFocus()

    def filter_actions(self, *_):
        if not hasattr(self, "action_specs"):
            return
        self.actions.clear()
        for name, shortcut, callback in self.action_specs:
            if self.action_search.text().casefold() in name.casefold():
                item = QListWidgetItem(name + ("    " + shortcut if shortcut else ""))
                item.setData(Qt.ItemDataRole.UserRole, callback)
                self.actions.addItem(item)
        self.actions.setCurrentRow(0)

    def execute_action(self, item=None):
        item = item or self.actions.currentItem()
        if item:
            callback = item.data(Qt.ItemDataRole.UserRole)
            self.close_actions()
            callback()

    def close_actions(self):
        if hasattr(self, "panel") and self.panel.isVisible():
            self.panel.hide()
            self.search.setFocus()

    def mutate(self, callback):
        callback()
        self.refresh()

    def copy_entry(self, entry):
        QApplication.clipboard().setText(entry.id.removeprefix("app:") if entry.id.startswith("app:") else entry.target)
        self.toast("Copied to clipboard")

    def set_alias(self, entry):
        self.close_actions()
        self.dialog_open = True
        try:
            alias, ok = QInputDialog.getText(self, "Set alias", "Search shortcut for " + entry.name + ":", text=self.catalog.state["aliases"].get(entry.id, ""))
            if ok:
                self.catalog.alias(entry, alias)
                self.refresh()
        finally:
            self.dialog_open = False
        self.search.setFocus()

    def present(self):
        self._opening = True
        self.catalog.reload()
        self.apply_theme()
        self.search.clear()
        self.refresh()
        self.history_index = -1
        self.brand.setText("◈  arch cast")
        screen = QApplication.screenAt(QCursor.pos()) or QApplication.primaryScreen()
        geometry = screen.availableGeometry()
        self.move(geometry.center().x() - self.width() // 2, geometry.center().y() - self.height() // 2)
        self.show()
        self.raise_()
        self.activateWindow()
        self.search.setFocus()
        # Wayland activation is compositor-owned. Explicitly focus only our launcher.
        QTimer.singleShot(80, self.focus_on_wayland)
        QTimer.singleShot(350, lambda: setattr(self, "_opening", False))

    def focus_on_wayland(self):
        import os
        if os.environ.get("HYPRLAND_INSTANCE_SIGNATURE") and self.isVisible():
            try:
                detached(["hyprctl", "dispatch", 'hl.dsp.focus({ window = "class:^(arch-cast)$" })'])
            except OSError:
                pass

    def closeEvent(self, event):
        event.ignore()
        self.hide()

    def eventFilter(self, watched, event):
        if watched is self and event.type() == QEvent.Type.WindowDeactivate:
            QTimer.singleShot(160, self.hide_if_unfocused)
        if event.type() == QEvent.Type.MouseButtonPress and self.panel.isVisible():
            if isinstance(watched, QWidget) and not self.panel.isAncestorOf(watched) and watched is not self.panel:
                self.close_actions()
        if event.type() != QEvent.Type.KeyPress or self.dialog_open:
            return False
        if not isinstance(watched, QWidget) or not (watched is self or self.isAncestorOf(watched)):
            return False
        key, mods = event.key(), event.modifiers()
        ctrl = bool(mods & Qt.KeyboardModifier.ControlModifier)
        shift = bool(mods & Qt.KeyboardModifier.ShiftModifier)
        alt = bool(mods & Qt.KeyboardModifier.AltModifier)
        entry = self.action_entry if self.panel.isVisible() else self.selected()
        if key == Qt.Key.Key_Escape:
            if self.panel.isVisible():
                self.close_actions()
            elif self.search.text() and not shift:
                self.search.clear()
            else:
                self.hide()
            return True
        if ctrl and key == Qt.Key.Key_K:
            self.toggle_actions()
            return True
        if ctrl and key == Qt.Key.Key_Comma:
            self.settings()
            return True
        if ctrl and shift and key == Qt.Key.Key_F and entry and entry.id.startswith(("app:", "custom:")):
            self.mutate(lambda: self.catalog.favorite(entry))
            return True
        if ctrl and shift and key == Qt.Key.Key_C and entry:
            self.copy_entry(entry)
            return True
        if alt and shift and key == Qt.Key.Key_D and entry and entry.id.startswith(("app:", "custom:")):
            self.mutate(lambda: self.catalog.hide(entry))
            return True
        if ctrl and key in (Qt.Key.Key_Plus, Qt.Key.Key_Equal, Qt.Key.Key_Minus, Qt.Key.Key_0):
            self.catalog.config["font_size"] = 12 if key == Qt.Key.Key_0 else max(10, min(20, self.font_size() + (-1 if key == Qt.Key.Key_Minus else 1)))
            self.catalog.save_config()
            self.apply_theme()
            return True
        if ctrl and Qt.Key.Key_1 <= key <= Qt.Key.Key_9:
            entries = [self.results.item(i).data(Qt.ItemDataRole.UserRole) for i in range(self.results.count())]
            entries = [e for e in entries if e]
            number = key - Qt.Key.Key_1
            if number < len(entries):
                self.run_entry(entries[number])
            return True
        if key in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
            if self.panel.isVisible():
                self.execute_action()
            else:
                self.run_entry(self.selected())
            return True
        delta = 0
        if key == Qt.Key.Key_Down or (ctrl and key == Qt.Key.Key_N):
            delta = 1
        elif key == Qt.Key.Key_Up or (ctrl and key == Qt.Key.Key_P):
            delta = -1
        if delta:
            if self.panel.isVisible():
                self.actions.setCurrentRow(max(0, min(self.actions.count() - 1, self.actions.currentRow() + delta)))
            elif delta == -1 and not self.search.text() and not ctrl and self.catalog.state["history"]:
                self.history_index = min(self.history_index + 1, len(self.catalog.state["history"]) - 1)
                self.search.setText(self.catalog.state["history"][self.history_index])
            else:
                self.navigate(delta, sections=ctrl and key in (Qt.Key.Key_Up, Qt.Key.Key_Down))
            return True
        return False

    def hide_if_unfocused(self):
        if self.isVisible() and not self._opening and not self.dialog_open and not self.isActiveWindow():
            self.close_actions()
            self.hide()
