import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from archcast.core import Catalog, Entry, MOCHA, calculate, discover_apps, write_json


class CatalogTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.config = Path(self.tmp.name) / "config"
        self.state = Path(self.tmp.name) / "state"
        self.apps = [Entry("app:firefox.desktop", "Firefox", keywords="browser web"), Entry("app:files.desktop", "Files"), Entry("app:terminal.desktop", "Ghostty", keywords="terminal shell")]
        self.catalog = Catalog(self.config, self.state, self.apps, clock=lambda: 1000000)

    def test_default_theme_is_mocha(self):
        self.assertEqual(self.catalog.theme(), MOCHA)

    def test_search_name_acronym_keywords_and_subsequence(self):
        for query, expected in [("fire", "Firefox"), ("ff", "Firefox"), ("browser", "Firefox"), ("ghsty", "Ghostty")]:
            self.assertEqual(self.catalog.search(query)[0].name, expected)

    def test_unmatched_queries_have_only_web_fallbacks(self):
        result = self.catalog.search("nonexistent-query-999")
        self.assertEqual([item.kind for item in result], ["Web Search", "Web Search"])
        self.assertIn("nonexistent-query-999", result[0].target)

    def test_query_is_encoded_not_executed(self):
        result = self.catalog.search("hello & goodbye; $HOME")[-1]
        self.assertIn("%26", result.target)
        self.assertIn("%24HOME", result.target)

    def test_alias_favorite_hide_and_persistence(self):
        firefox = self.apps[0]
        self.catalog.favorite(firefox)
        self.catalog.alias(firefox, "browse")
        self.assertEqual(self.catalog.search("")[0], firefox)
        self.assertEqual(self.catalog.search("browse")[0], firefox)
        self.catalog.hide(firefox)
        self.assertNotIn(firefox, self.catalog.search("browse"))
        loaded = Catalog(self.config, self.state, self.apps)
        self.assertIn(firefox.id, loaded.state["hidden"])
        self.assertEqual(loaded.state["aliases"][firefox.id], "browse")

    def test_learned_terms_and_reset(self):
        firefox = self.apps[0]
        self.catalog.record(firefox, "fi")
        self.assertEqual(self.catalog.search("fi")[0], firefox)
        self.assertEqual(self.catalog.state["usage"][firefox.id]["count"], 1)
        self.assertEqual(self.catalog.state["history"], ["fi"])
        self.catalog.reset_ranking(firefox)
        self.assertNotIn(firefox.id, self.catalog.state["usage"])
        self.assertNotIn("fi", self.catalog.state["learned"])

    def test_learned_terms_can_match_without_textual_overlap(self):
        firefox = self.apps[0]
        self.catalog.record(firefox, "work")
        self.assertEqual(self.catalog.search("work")[0], firefox)

    def test_theme_validation_and_config_preservation(self):
        self.catalog.config = {"theme": "Tokyo Night", "colors": {"accent": "#123456", "text": "bad", "unknown": "#ffffff"}, "commands": []}
        self.catalog.save_config()
        self.catalog.reload()
        self.assertEqual(self.catalog.theme()["accent"], "#123456")
        self.assertEqual(self.catalog.theme()["text"], "#c0caf5")
        self.assertNotIn("unknown", self.catalog.theme())

    def test_custom_commands_and_bad_entries(self):
        write_json(self.config / "config.json", {"commands": [{"id": "test", "name": "Test action", "command": "echo hi"}, {}, "bad"]})
        self.catalog.reload()
        result = self.catalog.search("Test action")[0]
        self.assertEqual(result.id, "custom:test")
        self.assertEqual(result.target, "echo hi")

    def test_corrupt_files_do_not_prevent_startup(self):
        self.config.mkdir()
        (self.config / "config.json").write_text("{broken")
        self.state.mkdir()
        (self.state / "state.json").write_text('{"favorites":false,"aliases":null}')
        self.catalog.reload()
        self.assertEqual(self.catalog.theme(), MOCHA)
        self.assertIsInstance(self.catalog.state["favorites"], list)
        self.assertIsInstance(self.catalog.state["aliases"], dict)

    def test_calculator_integrates_in_results(self):
        result = self.catalog.search("12 * (3 + 4)")[0]
        self.assertEqual(result.id, "calculator")
        self.assertEqual(result.target, "84")


class ArithmeticTests(unittest.TestCase):
    def test_valid(self):
        for expression, expected in [("2+2", "4"), ("= 10 / 4", "2.5"), ("2^8", "256"), ("-4 * 3", "-12"), ("5 % 2", "1"), ("(12 + 8) × 3", "60")]:
            self.assertEqual(calculate(expression), expected)

    def test_invalid_and_expensive(self):
        for expression in ["Firefox", "1/0", "__import__('os').system('true')", "9**999999999", "2**(2**100)", "[1]*1000000", "1e100*1e100", "(-1)**0.5", "True+1", "1+" ]:
            self.assertIsNone(calculate(expression), expression)


class DiscoveryTests(unittest.TestCase):
    def test_precedence_hidden_nested_and_filters(self):
        with tempfile.TemporaryDirectory() as tmp:
            user, system = Path(tmp) / "user", Path(tmp) / "system"
            user.mkdir()
            system.mkdir()
            def desktop(path, name, extra=""):
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(f"[Desktop Entry]\nType=Application\nName={name}\nExec=true\n{extra}")
            desktop(system / "browser.desktop", "System browser")
            desktop(user / "browser.desktop", "User browser")
            desktop(system / "hidden.desktop", "Should not appear")
            desktop(user / "hidden.desktop", "Hidden override", "Hidden=true\n")
            desktop(system / "private.desktop", "Private", "NoDisplay=true\n")
            desktop(system / "tools/editor.desktop", "Nested editor")
            desktop(system / "missing.desktop", "Missing", "TryExec=arch-cast-impossible-command-xyz\n")
            desktop(system / "kde.desktop", "KDE only", "OnlyShowIn=KDE;\n")
            with patch.dict(os.environ, XDG_CURRENT_DESKTOP="Hyprland"):
                apps = discover_apps([user, system])
            self.assertEqual({item.name for item in apps}, {"User browser", "Nested editor"})
            self.assertIn("app:tools-editor.desktop", [item.id for item in apps])


if __name__ == "__main__":
    unittest.main()
