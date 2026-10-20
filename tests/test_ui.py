"""Run headless with QT_QPA_PLATFORM=offscreen /usr/bin/python3 -m unittest discover -s tests."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
import tempfile
import unittest
from unittest.mock import patch

from PyQt6.QtCore import Qt
from PyQt6.QtTest import QTest
from PyQt6.QtWidgets import QApplication
from archcast.core import Catalog, Entry, MOCHA
from archcast.ui import Launcher, SettingsDialog

APP = QApplication.instance() or QApplication(["arch-cast-test"])


class LauncherTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.catalog = Catalog(self.tmp.name + "/config", self.tmp.name + "/state", [Entry("app:firefox.desktop", "Firefox"), Entry("app:terminal.desktop", "Ghostty")])
        self.window = Launcher(self.catalog)
        self.window.show()
        self.window.search.setFocus()
        APP.processEvents()

    def tearDown(self):
        APP.removeEventFilter(self.window)
        self.window.hide()
        self.window.deleteLater()
        APP.processEvents()
        self.tmp.cleanup()

    def key(self, key, mods=Qt.KeyboardModifier.NoModifier):
        QTest.keyClick(APP.focusWidget() or self.window.search, key, mods)
        APP.processEvents()

    def test_keyboard_search_actions_and_favorite(self):
        QTest.keyClicks(self.window.search, "firefox")
        self.assertEqual(self.window.selected().name, "Firefox")
        self.key(Qt.Key.Key_K, Qt.KeyboardModifier.ControlModifier)
        self.assertTrue(self.window.panel.isVisible())
        QTest.keyClicks(self.window.action_search, "favorite")
        self.assertEqual(self.window.actions.count(), 1)
        self.key(Qt.Key.Key_Return)
        self.assertIn("app:firefox.desktop", self.catalog.state["favorites"])
        self.assertFalse(self.window.panel.isVisible())

    def test_escape_closes_panel_then_query_then_window(self):
        self.window.search.setText("firefox")
        self.window.toggle_actions()
        self.key(Qt.Key.Key_Escape)
        self.assertFalse(self.window.panel.isVisible())
        self.key(Qt.Key.Key_Escape)
        self.assertEqual(self.window.search.text(), "")
        self.key(Qt.Key.Key_Escape)
        self.assertFalse(self.window.isVisible())

    def test_calculator_copies_and_stays_open(self):
        self.window.search.setText("6 * 7")
        self.key(Qt.Key.Key_Return)
        self.assertEqual(APP.clipboard().text(), "42")
        self.assertTrue(self.window.isVisible())

    @patch("archcast.ui.detached")
    def test_app_launch_is_detached_and_records_usage(self, detached):
        self.window.search.setText("firefox")
        self.key(Qt.Key.Key_Return)
        detached.assert_called_once_with(["gtk-launch", "firefox.desktop"])
        self.assertFalse(self.window.isVisible())
        self.assertEqual(self.catalog.state["usage"]["app:firefox.desktop"]["count"], 1)

    def test_navigation_skips_sections_and_quick_pick(self):
        self.window.search.setText("unmatched")
        self.assertEqual(self.window.selected().id, "web:google")
        self.key(Qt.Key.Key_Down)
        self.assertEqual(self.window.selected().id, "web:duckduckgo")
        self.key(Qt.Key.Key_Up)
        self.assertEqual(self.window.selected().id, "web:google")

    def test_settings_preview_cancel_and_save(self):
        dialog = SettingsDialog(self.window)
        dialog.preset.setCurrentText("Catppuccin Latte")
        self.assertEqual(self.catalog.theme()["background"], "#eff1f5")
        dialog.reject()
        self.assertEqual(self.catalog.theme(), MOCHA)
        dialog = SettingsDialog(self.window)
        dialog.colors["accent"].setText("#abcdef")
        dialog.save()
        self.catalog.reload()
        self.assertEqual(self.catalog.theme()["accent"], "#abcdef")
        self.assertEqual(self.catalog.config["theme"], "Catppuccin Mocha")

    def test_hidden_results_restore(self):
        entry = self.catalog.entries[0]
        self.catalog.hide(entry)
        self.window.refresh()
        self.assertNotIn(entry, self.catalog.search("firefox"))
        dialog = SettingsDialog(self.window)
        from PyQt6.QtWidgets import QPushButton
        button = QPushButton()
        dialog.restore_hidden(button)
        self.assertIn(entry, self.catalog.search("firefox"))
        dialog.reject()


if __name__ == "__main__":
    unittest.main()
