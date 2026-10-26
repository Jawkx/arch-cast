use super::Launcher;
use arch_cast::{
    catalog::{Catalog, Config, Entry},
    paths::Paths,
};
use gpui::test::TestWindowExt;
use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};

fn harness(
    cx: &mut TestAppContext,
) -> (tempfile::TempDir, Entity<Launcher>, &mut VisualTestContext) {
    let temp = tempfile::tempdir().unwrap();
    let paths = Paths {
        config: temp.path().join("config"),
        state: temp.path().join("state"),
        runtime: temp.path().join("runtime"),
        data: temp.path().join("data"),
        home: temp.path().into(),
    };
    let catalog = Catalog::with_applications(
        paths,
        Some(vec![
            Entry::app("app:firefox.desktop", "Firefox", "browser web"),
            Entry::app("app:terminal.desktop", "Ghostty", "terminal"),
        ]),
    );
    cx.update(gpui::init);
    let mut launcher = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Launcher::new(catalog, window, cx));
        launcher = Some(view.clone());
        gpui::base::Root::new(view, window, cx)
    });
    cx.update(|window, cx| window.render_frame(cx));
    (temp, launcher.unwrap(), cx)
}

#[gpui::test]
fn keyboard_search_actions_and_favorite(cx: &mut TestAppContext) {
    let (_temp, view, cx) = harness(cx);
    cx.simulate_input("firefox");
    cx.cx
        .read(|cx| assert_eq!(view.read(cx).results[0].name, "Firefox"));
    cx.simulate_keystrokes("ctrl-k");
    cx.simulate_input("favorite");
    cx.cx
        .read(|cx| assert_eq!(view.read(cx).action_list(cx).len(), 1));
    cx.simulate_keystrokes("enter");
    cx.cx.read(|cx| {
        assert!(
            view.read(cx)
                .catalog
                .state
                .favorites
                .contains(&"app:firefox.desktop".into())
        );
        assert!(view.read(cx).actions.is_none());
    });
}

#[gpui::test]
fn escape_closes_actions_then_query_then_window(cx: &mut TestAppContext) {
    let (_temp, view, cx) = harness(cx);
    cx.simulate_input("firefox");
    cx.simulate_keystrokes("ctrl-k escape");
    cx.cx.read(|cx| {
        assert!(view.read(cx).actions.is_none());
        assert_eq!(view.read(cx).query, "firefox");
    });
    cx.simulate_keystrokes("escape");
    cx.cx.read(|cx| assert!(view.read(cx).query.is_empty()));
    cx.simulate_keystrokes("escape");
    cx.cx.read(|cx| assert!(cx.windows().is_empty()));
}

#[gpui::test]
fn calculator_copies_and_keeps_window_open(cx: &mut TestAppContext) {
    let (_temp, view, cx) = harness(cx);
    cx.simulate_input("6 * 7");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some("42")
    );
    cx.cx.read(|cx| {
        assert!(!cx.windows().is_empty());
        assert_eq!(view.read(cx).results[0].id, "calculator");
    });
}

#[gpui::test]
fn navigation_and_section_jumps_skip_headers(cx: &mut TestAppContext) {
    let (_temp, view, cx) = harness(cx);
    cx.simulate_input("unmatched");
    cx.simulate_keystrokes("down");
    cx.cx.read(|cx| {
        assert_eq!(
            view.read(cx).results[view.read(cx).selected].id,
            "web:duckduckgo"
        )
    });
    cx.simulate_keystrokes("ctrl-p");
    cx.cx.read(|cx| assert_eq!(view.read(cx).selected, 0));
    cx.simulate_keystrokes("escape");
    cx.simulate_input("6 * 7");
    cx.simulate_keystrokes("ctrl-down");
    cx.cx.read(|cx| {
        assert_eq!(
            view.read(cx).results[view.read(cx).selected].id,
            "web:google"
        )
    });
}

#[gpui::test]
fn history_recall_copy_id_and_numbered_calculator_pick(cx: &mut TestAppContext) {
    let (_temp, view, cx) = harness(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let entry = view.catalog.entries[0].clone();
            view.catalog.record(&entry, "work").unwrap();
        })
    });
    cx.simulate_keystrokes("up");
    cx.cx.read(|cx| {
        assert_eq!(view.read(cx).query, "work");
        assert_eq!(view.read(cx).results[0].name, "Firefox");
    });
    cx.simulate_keystrokes("ctrl-shift-c");
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some("firefox.desktop")
    );
    cx.simulate_keystrokes("escape");
    cx.simulate_input("2+2");
    cx.simulate_keystrokes("ctrl-1");
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some("4")
    );
}

#[gpui::test]
fn alias_editor_saves_and_can_remove_alias(cx: &mut TestAppContext) {
    let (_temp, view, cx) = harness(cx);
    cx.simulate_input("firefox");
    cx.simulate_keystrokes("ctrl-k");
    cx.simulate_input("alias");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("browse");
    cx.simulate_keystrokes("enter");
    cx.cx.read(|cx| {
        assert_eq!(
            view.read(cx).catalog.state.aliases["app:firefox.desktop"],
            "browse"
        )
    });
    cx.simulate_keystrokes("ctrl-k");
    cx.simulate_input("alias");
    cx.simulate_keystrokes("enter ctrl-a backspace enter");
    cx.cx.read(|cx| {
        assert!(
            !view
                .read(cx)
                .catalog
                .state
                .aliases
                .contains_key("app:firefox.desktop")
        )
    });
}

#[gpui::test]
fn settings_preview_cancel_save_and_validation(cx: &mut TestAppContext) {
    let (_temp, view, cx) = harness(cx);
    cx.simulate_keystrokes("ctrl-,");
    cx.update(|window, cx| window.click(("preset", 2usize), cx));
    cx.cx.read(|cx| {
        assert_eq!(
            gpui::component::Theme::global(cx).background,
            gpui::Hsla::from(gpui::rgb(0xeff1f5))
        );
        assert_eq!(view.read(cx).catalog.config.theme, "Catppuccin Mocha");
    });
    cx.simulate_keystrokes("escape");
    cx.cx.read(|cx| {
        assert_eq!(
            gpui::component::Theme::global(cx).background,
            gpui::Hsla::from(gpui::rgb(0x1e1e2e))
        )
    });
    cx.simulate_keystrokes("ctrl-,");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let input = view.settings.as_ref().unwrap().colors[6].clone();
            input.update(cx, |input, cx| input.set_value("#abcdef", window, cx));
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-s");
    cx.cx.read(|cx| {
        let view = view.read(cx);
        assert!(view.settings.is_none());
        let config = Config::load(&view.catalog.paths.config.join("config.json"));
        assert_eq!(config.palette().get("accent"), 0xabcdef);
    });
    cx.simulate_keystrokes("ctrl-,");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let input = view.settings.as_ref().unwrap().width.clone();
            input.update(cx, |input, cx| input.set_value("invalid", window, cx));
        })
    });
    cx.simulate_keystrokes("ctrl-s");
    cx.cx.read(|cx| assert!(view.read(cx).settings.is_some()));
}

#[gpui::test]
fn settings_restore_hidden_results_and_favorite_shortcut(cx: &mut TestAppContext) {
    let (_temp, view, cx) = harness(cx);
    cx.simulate_input("firefox");
    cx.simulate_keystrokes("ctrl-shift-f");
    cx.cx.read(|cx| {
        assert!(
            view.read(cx)
                .catalog
                .state
                .favorites
                .contains(&"app:firefox.desktop".into())
        )
    });
    cx.simulate_keystrokes("alt-shift-d");
    cx.cx.read(|cx| {
        assert!(
            view.read(cx)
                .catalog
                .state
                .hidden
                .contains(&"app:firefox.desktop".into())
        )
    });
    cx.simulate_keystrokes("ctrl-,");
    cx.update(|window, cx| window.click("restore-hidden", cx));
    cx.simulate_keystrokes("escape");
    cx.cx.read(|cx| {
        assert!(view.read(cx).catalog.state.hidden.is_empty());
        assert_eq!(view.read(cx).catalog.search("firefox")[0].name, "Firefox");
        assert_eq!(view.read(cx).results[0].name, "Firefox");
    });
}
