use arch_cast::{
    catalog::{COLOR_KEYS, Catalog, Config, Entry, EntryKind, THEME_NAMES, parse_color},
    platform,
};
use gpui::component::{
    Colorize, Sizable, Theme, ThemeMode,
    button::{Button, ButtonVariants},
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    input::{Input, InputEvent, InputState},
};
use gpui::{prelude::*, *};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Open,
    Favorite,
    Alias,
    Copy,
    Reset,
    Hide,
    Settings,
}
struct SettingsEditor {
    draft: Config,
    colors: Vec<Entity<InputState>>,
    pickers: Vec<Entity<ColorPickerState>>,
    font: Entity<InputState>,
    font_size: Entity<InputState>,
    width: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

pub struct Launcher {
    catalog: Catalog,
    search: Entity<InputState>,
    action_input: Entity<InputState>,
    query: String,
    results: Vec<Entry>,
    selected: usize,
    actions: Option<Entry>,
    action_selected: usize,
    alias: Option<Entry>,
    settings: Option<SettingsEditor>,
    scroll: ScrollHandle,
    action_scroll: ScrollHandle,
    icons: HashMap<String, Option<PathBuf>>,
    message: Option<(String, Instant)>,
    opened: Instant,
    history_index: usize,
    _subscriptions: Vec<Subscription>,
}

pub fn apply_theme(config: &Config, window: &mut Window, cx: &mut App) {
    let p = config.palette();
    Theme::change(
        if config.theme == "Catppuccin Latte" {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        },
        Some(window),
        cx,
    );
    let t = Theme::global_mut(cx);
    let color = |key| Hsla::from(rgb(p.get(key)));
    t.font_family = config.font.clone().into();
    t.font_size = px(config.font_size as f32 * 4. / 3.);
    t.background = color("background");
    t.foreground = color("text");
    t.border = color("border");
    t.input = color("surface");
    t.caret = color("accent");
    t.ring = color("accent");
    t.primary = color("accent");
    t.primary_foreground = color("background");
    t.muted = color("selection");
    t.muted_foreground = color("muted");
    t.accent = color("selection");
    t.accent_foreground = color("text");
    t.popover = color("surface");
    t.popover_foreground = color("text");
    t.colors.list = color("background");
    t.list_active = color("selection");
    t.list_hover = color("surface");
    t.button = color("selection");
    t.button_foreground = color("text");
    t.button_hover = color("surface");
    t.selection = color("selection");
    window.resize(size(px(config.width as f32), px(584.)));
    window.refresh();
}

impl Launcher {
    pub fn new(catalog: Catalog, window: &mut Window, cx: &mut Context<Self>) -> Self {
        window.set_window_title("Arch Cast");
        apply_theme(&catalog.config, window, cx);
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search apps and commands…"));
        let action_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search actions…"));
        search.update(cx, |s, cx| s.focus(window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&search, window, |this, input, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.query = input.read(cx).value().to_string();
                    this.actions = None;
                    this.history_index = 0;
                    this.refresh(cx);
                }
            }),
            cx.subscribe_in(
                &action_input,
                window,
                |this, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.action_selected = 0;
                        this.action_scroll.set_offset(point(px(0.), px(0.)));
                        cx.notify();
                    }
                },
            ),
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    let delay = Duration::from_millis(350).saturating_sub(this.opened.elapsed());
                    cx.spawn_in(window, async move |_, cx| {
                        smol::Timer::after(delay.max(Duration::from_millis(100))).await;
                        let _ = cx.update(|window, _| {
                            if !window.is_window_active() {
                                window.remove_window();
                            }
                        });
                    })
                    .detach();
                }
            }),
        ];
        let results = catalog.search("");
        let mut this = Self {
            catalog,
            search,
            action_input,
            query: String::new(),
            results,
            selected: 0,
            actions: None,
            action_selected: 0,
            alias: None,
            settings: None,
            scroll: ScrollHandle::new(),
            action_scroll: ScrollHandle::new(),
            icons: HashMap::new(),
            message: None,
            opened: Instant::now(),
            history_index: 0,
            _subscriptions: subscriptions,
        };
        this.load_icons();
        #[cfg(not(test))]
        cx.spawn_in(window, async move |_, cx| {
            smol::Timer::after(Duration::from_millis(100)).await;
            let _ = cx.update(|window, _| {
                window.activate_window();
                platform::focus_launcher();
            });
        })
        .detach();
        this
    }
    fn load_icons(&mut self) {
        for entry in &self.results {
            self.icons
                .entry(entry.icon.clone())
                .or_insert_with(|| platform::icon_path(&entry.icon));
        }
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.results = self.catalog.search(&self.query);
        self.selected = 0;
        self.scroll.set_offset(point(px(0.), px(0.)));
        self.load_icons();
        cx.notify();
    }
    fn focus_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |s, cx| s.focus(window, cx));
    }
    fn close_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.actions = None;
        self.alias = None;
        self.focus_search(window, cx);
        cx.notify();
    }
    fn toast(&mut self, message: impl Into<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.message = Some((message.into(), Instant::now()));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            smol::Timer::after(Duration::from_secs(3)).await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .message
                    .as_ref()
                    .is_some_and(|(_, at)| at.elapsed() >= Duration::from_secs(3))
                {
                    this.message = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    fn report(
        &mut self,
        result: anyhow::Result<()>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Err(error) = result {
            eprintln!("arch-cast: {error:#}");
            self.toast(format!("Could not save: {error}"), window, cx);
            false
        } else {
            true
        }
    }
    pub fn run_id(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.catalog.entries.iter().find(|e| e.id == id).cloned() {
            self.run_entry(entry, window, cx);
        } else {
            self.toast(format!("Unknown entry: {id}"), window, cx);
        }
    }
    fn run_entry(&mut self, entry: Entry, window: &mut Window, cx: &mut Context<Self>) {
        self.actions = None;
        match entry.id.as_str() {
            "settings" => self.open_settings(window, cx),
            "reload" => {
                self.catalog.reload();
                apply_theme(&self.catalog.config, window, cx);
                self.refresh(cx);
                self.focus_search(window, cx);
                self.toast("Applications refreshed", window, cx);
            }
            "calculator" => {
                cx.write_to_clipboard(ClipboardItem::new_string(entry.target.clone()));
                self.focus_search(window, cx);
                self.toast(format!("Copied {}", entry.target), window, cx);
            }
            _ => match platform::launch(&entry, &self.catalog) {
                Ok(()) => {
                    let result = self.catalog.record(&entry, &self.query);
                    if self.report(result, window, cx) {
                        window.remove_window();
                    }
                }
                Err(error) => {
                    self.toast(format!("Could not launch: {error:#}"), window, cx);
                    self.focus_search(window, cx);
                }
            },
        }
    }
    fn selected_entry(&self) -> Option<Entry> {
        self.actions
            .clone()
            .or_else(|| self.results.get(self.selected).cloned())
    }
    fn toggle_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.actions.is_some() {
            self.close_actions(window, cx);
            return;
        }
        if let Some(entry) = self.results.get(self.selected).cloned() {
            self.actions = Some(entry);
            self.action_selected = 0;
            self.action_input.update(cx, |s, cx| {
                s.set_value("", window, cx);
                s.focus(window, cx);
            });
            cx.notify();
        }
    }
    fn action_list(&self, cx: &App) -> Vec<(Action, String, &'static str)> {
        let Some(entry) = &self.actions else {
            return vec![];
        };
        let mut actions: Vec<(Action, String, &'static str)> = vec![(
            Action::Open,
            if entry.id == "calculator" {
                "Copy result"
            } else {
                "Open"
            }
            .into(),
            "↵",
        )];
        if entry.customizable() {
            actions.extend([
                (
                    Action::Favorite,
                    if self.catalog.state.favorites.contains(&entry.id) {
                        "Remove from Favorites"
                    } else {
                        "Add to Favorites"
                    }
                    .into(),
                    "Ctrl Shift F",
                ),
                (Action::Alias, "Set Alias".into(), ""),
                (
                    Action::Copy,
                    if entry.id.starts_with("app:") {
                        "Copy Desktop ID"
                    } else {
                        "Copy Command"
                    }
                    .into(),
                    "Ctrl Shift C",
                ),
                (Action::Reset, "Reset Ranking".into(), ""),
                (Action::Hide, "Hide Result".into(), "Alt Shift D"),
            ]);
        }
        actions.push((Action::Settings, "Launcher Settings".into(), "Ctrl ,"));
        let filter = self.action_input.read(cx).value().to_lowercase();
        actions
            .into_iter()
            .filter(|(_, name, _)| name.to_lowercase().contains(&filter))
            .collect()
    }
    fn execute(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        self.close_actions(window, cx);
        let result = match action {
            Action::Open => {
                self.run_entry(entry, window, cx);
                return;
            }
            Action::Settings => {
                self.open_settings(window, cx);
                return;
            }
            Action::Favorite => self.catalog.favorite(&entry),
            Action::Hide => self.catalog.hide(&entry),
            Action::Reset => self.catalog.reset_ranking(&entry),
            Action::Copy => {
                cx.write_to_clipboard(ClipboardItem::new_string(
                    entry
                        .id
                        .strip_prefix("app:")
                        .unwrap_or(&entry.target)
                        .into(),
                ));
                self.toast("Copied to clipboard", window, cx);
                return;
            }
            Action::Alias => {
                let value = self
                    .catalog
                    .state
                    .aliases
                    .get(&entry.id)
                    .cloned()
                    .unwrap_or_default();
                self.alias = Some(entry);
                self.action_input.update(cx, |s, cx| {
                    s.set_value(value, window, cx);
                    s.focus(window, cx);
                });
                cx.notify();
                return;
            }
        };
        self.report(result, window, cx);
        self.refresh(cx);
    }
    fn save_alias(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.alias.clone() {
            let value = self.action_input.read(cx).value();
            let result = self.catalog.alias(&entry, &value);
            if self.report(result, window, cx) {
                self.close_actions(window, cx);
                self.refresh(cx);
            }
        }
    }
    fn navigate(&mut self, delta: i32, sections: bool, cx: &mut Context<Self>) {
        if self.actions.is_some() {
            let count = self.action_list(cx).len();
            self.action_selected = (self.action_selected as i32 + delta)
                .clamp(0, count.saturating_sub(1) as i32)
                as usize;
            self.action_scroll.scroll_to_item(self.action_selected);
        } else if !self.results.is_empty() {
            let old = self.selected;
            let mut next = old as i32 + delta;
            while next >= 0 && (next as usize) < self.results.len() {
                let n = next as usize;
                if !sections
                    || self.catalog.section(&self.results[n], &self.query)
                        != self
                            .catalog
                            .section(&self.results[(next - delta) as usize], &self.query)
                {
                    self.selected = n;
                    break;
                }
                next += delta;
            }
            let mut row = self.selected;
            let mut last = "";
            for e in self.results.iter().take(self.selected + 1) {
                let section = self.catalog.section(e, &self.query);
                if section != last {
                    row += 1;
                    last = section;
                }
            }
            self.scroll.scroll_to_item(row);
        }
        cx.notify();
    }
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.to_lowercase();
        let mods = event.keystroke.modifiers;
        if self.settings.is_some() {
            if mods.control && key == "s" {
                self.save_settings(window, cx);
                cx.stop_propagation();
            }
            return;
        }
        if self.alias.is_some() {
            match key.as_str() {
                "enter" => self.save_alias(window, cx),
                "escape" => self.close_actions(window, cx),
                _ => return,
            }
        } else if key == "escape" {
            if mods.shift {
                window.remove_window();
            } else if self.actions.is_some() {
                self.close_actions(window, cx);
            } else if !self.query.is_empty() {
                self.search.update(cx, |s, cx| s.set_value("", window, cx));
                self.query.clear();
                self.refresh(cx);
            } else {
                window.remove_window();
            }
        } else if mods.control && key == "k" {
            self.toggle_actions(window, cx);
        } else if mods.control && key == "," {
            self.open_settings(window, cx);
        } else if mods.control && mods.shift && key == "f" {
            self.execute(Action::Favorite, window, cx);
        } else if mods.control && mods.shift && key == "c" {
            self.execute(Action::Copy, window, cx);
        } else if mods.alt && mods.shift && key == "d" {
            self.execute(Action::Hide, window, cx);
        } else if mods.control && matches!(key.as_str(), "+" | "=" | "-" | "0") {
            self.catalog.config.font_size = match key.as_str() {
                "0" => 12,
                "-" => self.catalog.config.font_size.saturating_sub(1).max(10),
                _ => (self.catalog.config.font_size + 1).min(20),
            };
            let result = self.catalog.save_config();
            self.report(result, window, cx);
            apply_theme(&self.catalog.config, window, cx);
        } else if mods.control && key.len() == 1 && matches!(key.as_bytes()[0], b'1'..=b'9') {
            if let Some(entry) = self
                .results
                .get((key.as_bytes()[0] - b'1') as usize)
                .cloned()
            {
                self.run_entry(entry, window, cx);
            }
        } else if key == "enter" {
            if self.actions.is_some() {
                if let Some((action, _, _)) = self.action_list(cx).get(self.action_selected) {
                    self.execute(*action, window, cx);
                }
            } else if let Some(entry) = self.results.get(self.selected).cloned() {
                self.run_entry(entry, window, cx);
            }
        } else if key == "up"
            && !mods.control
            && self.actions.is_none()
            && self.query.is_empty()
            && !self.catalog.state.history.is_empty()
        {
            let query = self.catalog.state.history
                [self.history_index.min(self.catalog.state.history.len() - 1)]
            .clone();
            self.history_index += 1;
            self.query = query.clone();
            self.search
                .update(cx, |s, cx| s.set_value(query, window, cx));
            self.refresh(cx);
        } else if key == "down" || (mods.control && key == "n") {
            self.navigate(1, mods.control && key == "down", cx);
        } else if key == "up" || (mods.control && key == "p") {
            self.navigate(-1, mods.control && key == "up", cx);
        } else {
            return;
        }
        cx.stop_propagation();
    }

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.actions = None;
        self.alias = None;
        let draft = self.catalog.config.clone();
        let palette = draft.palette();
        let mut colors = vec![];
        let mut pickers = vec![];
        let mut subscriptions = vec![];
        for key in COLOR_KEYS {
            let value = format!("#{:06x}", palette.get(key));
            let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
            let picker =
                cx.new(|cx| ColorPickerState::new(window, cx).default_value(rgb(palette.get(key))));
            let input_for_picker = input.clone();
            subscriptions.push(cx.subscribe_in(
                &picker,
                window,
                move |this, _, event: &ColorPickerEvent, window, cx| {
                    let ColorPickerEvent::Change(Some(value)) = event else {
                        return;
                    };
                    input_for_picker.update(cx, |s, cx| s.set_value(value.to_hex(), window, cx));
                    this.preview_settings(window, cx);
                },
            ));
            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.preview_settings(window, cx);
                    }
                },
            ));
            colors.push(input);
            pickers.push(picker);
        }
        let font = cx.new(|cx| InputState::new(window, cx).default_value(draft.font.clone()));
        let font_size =
            cx.new(|cx| InputState::new(window, cx).default_value(draft.font_size.to_string()));
        let width = cx.new(|cx| InputState::new(window, cx).default_value(draft.width.to_string()));
        for input in [&font, &font_size, &width] {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.preview_settings(window, cx);
                    }
                },
            ));
        }
        self.settings = Some(SettingsEditor {
            draft,
            colors,
            pickers,
            font,
            font_size,
            width,
            _subscriptions: subscriptions,
        });
        cx.notify();
        if let Some(settings) = &self.settings {
            settings.font.update(cx, |s, cx| s.focus(window, cx));
        }
    }
    fn settings_config(&self, cx: &App) -> anyhow::Result<Config> {
        let editor = self.settings.as_ref().unwrap();
        let mut config = editor.draft.clone();
        config.font = editor.font.read(cx).value().trim().into();
        anyhow::ensure!(!config.font.is_empty(), "Font cannot be empty");
        config.font_size = editor.font_size.read(cx).value().parse()?;
        anyhow::ensure!(
            (10..=20).contains(&config.font_size),
            "Font size must be 10 to 20"
        );
        config.width = editor.width.read(cx).value().parse()?;
        anyhow::ensure!(
            (640..=1200).contains(&config.width),
            "Window width must be 640 to 1200"
        );
        for (i, key) in COLOR_KEYS.iter().enumerate() {
            let value = editor.colors[i].read(cx).value().to_string();
            anyhow::ensure!(
                parse_color(&value).is_some(),
                "{key} must be a six-digit hex color"
            );
            config.colors.insert((*key).into(), value);
        }
        Ok(config)
    }
    fn preview_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_none() {
            return;
        }
        if let Ok(config) = self.settings_config(cx) {
            if let Some(editor) = &self.settings {
                for (i, key) in COLOR_KEYS.iter().enumerate() {
                    let color = Hsla::from(rgb(config.palette().get(key)));
                    if editor.pickers[i].read(cx).value() != Some(color) {
                        editor.pickers[i].update(cx, |s, cx| s.set_value(color, window, cx));
                    }
                }
            }
            apply_theme(&config, window, cx);
        }
        cx.notify();
    }
    fn select_preset(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = &mut self.settings {
            editor.draft.theme = THEME_NAMES[index].into();
            editor.draft.colors.clear();
            let palette = editor.draft.palette();
            for (i, key) in COLOR_KEYS.iter().enumerate() {
                editor.colors[i].update(cx, |s, cx| {
                    s.set_value(format!("#{:06x}", palette.get(key)), window, cx)
                });
                editor.pickers[i]
                    .update(cx, |s, cx| s.set_value(rgb(palette.get(key)), window, cx));
            }
        }
        self.preview_settings(window, cx);
    }
    fn save_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.settings_config(cx) {
            Ok(config) => {
                let original = std::mem::replace(&mut self.catalog.config, config);
                let result = self.catalog.save_config();
                if self.report(result, window, cx) {
                    self.settings = None;
                    self.refresh(cx);
                    self.focus_search(window, cx);
                } else {
                    self.catalog.config = original;
                }
            }
            Err(error) => self.toast(error.to_string(), window, cx),
        }
    }
    fn cancel_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings = None;
        apply_theme(&self.catalog.config, window, cx);
        self.refresh(cx);
        self.focus_search(window, cx);
    }

    fn render_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let editor = self.settings.as_ref().unwrap();
        let p = self
            .settings_config(cx)
            .unwrap_or_else(|_| editor.draft.clone())
            .palette();
        let error = self.settings_config(cx).err().map(|e| e.to_string());
        let mut colors = div().flex().flex_col().gap_2();
        for (i, key) in COLOR_KEYS.iter().enumerate() {
            colors = colors.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().w(px(120.)).child(*key))
                    .child(
                        div()
                            .w(px(150.))
                            .child(Input::new(&editor.colors[i]).small()),
                    )
                    .child(ColorPicker::new(&editor.pickers[i]).small()),
            );
        }
        let mut presets = div().flex().gap_2();
        for (i, name) in THEME_NAMES.iter().enumerate() {
            presets = presets.child(
                Button::new(("preset", i))
                    .small()
                    .label(name.split_whitespace().last().unwrap_or(name).to_owned())
                    .when(editor.draft.theme == *name, |b| b.primary())
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.select_preset(i, window, cx)),
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .px_6()
                    .py_4()
                    .border_b_1()
                    .border_color(rgb(p.get("border")))
                    .child(div().text_xl().child("Launcher settings"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(p.get("muted")))
                            .child("Changes preview live. Save to keep them."),
                    ),
            )
            .child(
                div()
                    .id("settings-scroll")
                    .overflow_y_scroll()
                    .flex_1()
                    .min_h_0()
                    .px_6()
                    .py_4()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(presets)
                    .child(colors)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(div().w(px(120.)).child("Font"))
                            .child(div().flex_1().child(Input::new(&editor.font).small())),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(div().w(px(120.)).child("Font size"))
                            .child(
                                div()
                                    .w(px(80.))
                                    .child(Input::new(&editor.font_size).small()),
                            )
                            .child("10–20 pt")
                            .child(div().ml_4().child("Width"))
                            .child(div().w(px(90.)).child(Input::new(&editor.width).small()))
                            .child("640–1200 px"),
                    )
                    .child(
                        Button::new("restore-hidden")
                            .small()
                            .label(format!(
                                "Restore hidden results ({})",
                                self.catalog.state.hidden.len()
                            ))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.catalog.state.hidden.clear();
                                let result = this.catalog.save_state();
                                this.report(result, window, cx);
                                cx.notify();
                            })),
                    )
                    .when_some(error, |d, error| {
                        d.child(div().text_sm().text_color(rgb(0xf38ba8)).child(error))
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_6()
                    .py_3()
                    .border_t_1()
                    .border_color(rgb(p.get("border")))
                    .child(
                        div().text_sm().text_color(rgb(p.get("muted"))).child(
                            self.message
                                .as_ref()
                                .map(|(s, _)| s.clone())
                                .unwrap_or_else(|| "Ctrl+S to save · Esc to cancel".into()),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(Button::new("cancel-settings").label("Cancel").on_click(
                                cx.listener(|this, _, window, cx| this.cancel_settings(window, cx)),
                            ))
                            .child(
                                Button::new("save-settings")
                                    .primary()
                                    .label("Save")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.save_settings(window, cx)
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
    fn render_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let entry = self.actions.as_ref().unwrap();
        let p = self.catalog.config.palette();
        let mut list = div()
            .id("action-list")
            .max_h(px(300.))
            .overflow_y_scroll()
            .track_scroll(&self.action_scroll)
            .flex()
            .flex_col()
            .gap_1();
        for (i, (action, name, shortcut)) in self.action_list(cx).into_iter().enumerate() {
            list = list.child(
                div()
                    .id(("action", i))
                    .flex()
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .when(i == self.action_selected, |d| d.bg(rgb(p.get("selection"))))
                    .hover(|d| d.bg(rgb(p.get("selection"))))
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.execute(action, window, cx)),
                    )
                    .child(name)
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(p.get("muted")))
                            .child(shortcut),
                    ),
            );
        }
        div()
            .absolute()
            .right_4()
            .bottom(px(60.))
            .w(px(380.))
            .rounded_lg()
            .shadow_lg()
            .border_1()
            .border_color(rgb(p.get("border")))
            .bg(rgb(p.get("surface")))
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .px_3()
                    .py_1()
                    .text_sm()
                    .text_color(rgb(p.get("muted")))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(entry.name.clone()),
            )
            .child(list)
            .child(Input::new(&self.action_input).small())
            .into_any_element()
    }
    fn render_alias(&self, cx: &mut Context<Self>) -> AnyElement {
        let entry = self.alias.as_ref().unwrap();
        let p = self.catalog.config.palette();
        div()
            .absolute()
            .inset_0()
            .bg(gpui::black().opacity(0.5))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(440.))
                    .p_6()
                    .rounded_lg()
                    .bg(rgb(p.get("surface")))
                    .border_1()
                    .border_color(rgb(p.get("border")))
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(format!("Search alias for {}", entry.name))
                    .child(Input::new(&self.action_input))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(p.get("muted")))
                            .child("Leave empty to remove the alias."),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(Button::new("cancel-alias").label("Cancel").on_click(
                                cx.listener(|this, _, window, cx| this.close_actions(window, cx)),
                            ))
                            .child(Button::new("save-alias").primary().label("Save").on_click(
                                cx.listener(|this, _, window, cx| this.save_alias(window, cx)),
                            )),
                    ),
            )
            .into_any_element()
    }
}

impl Render for Launcher {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let config = if self.settings.is_some() {
            self.settings_config(cx)
                .unwrap_or_else(|_| self.catalog.config.clone())
        } else {
            self.catalog.config.clone()
        };
        let p = config.palette();
        let shell = div()
            .id("launcher")
            .role(Role::Group)
            .aria_label("Arch Cast launcher")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(14.))
            .bg(rgb(p.get("background")))
            .text_color(rgb(p.get("text")))
            .font_family(config.font)
            .text_size(px(config.font_size as f32 * 4. / 3.))
            .border_1()
            .border_color(rgb(p.get("border")))
            .capture_key_down(cx.listener(Self::key_down))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.settings.is_some() && event.keystroke.key == "escape" {
                    this.cancel_settings(window, cx);
                    cx.stop_propagation();
                }
            }));
        if self.settings.is_some() {
            return shell.child(self.render_settings(cx));
        }
        let mut list = div()
            .id("results")
            .role(Role::ListBox)
            .aria_label("Search results")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p_2()
            .flex()
            .flex_col();
        let mut previous = "";
        for (i, entry) in self.results.iter().enumerate() {
            let section = self.catalog.section(entry, &self.query);
            if section != previous {
                list = list.child(
                    div()
                        .h(px(32.))
                        .flex_shrink_0()
                        .px_4()
                        .flex()
                        .items_center()
                        .text_sm()
                        .text_color(rgb(p.get("muted")))
                        .child(section),
                );
                previous = section;
            }
            let icon = if let Some(Some(path)) = self.icons.get(&entry.icon) {
                img(path.clone()).size(px(28.)).into_any_element()
            } else {
                div()
                    .size(px(28.))
                    .rounded_md()
                    .bg(rgb(p.get("selection")))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(p.get("accent")))
                    .child(match entry.kind {
                        EntryKind::Calculator => "=",
                        EntryKind::WebSearch => "↗",
                        _ => "◇",
                    })
                    .into_any_element()
            };
            let alias = self
                .catalog
                .state
                .aliases
                .get(&entry.id)
                .unwrap_or(&entry.detail);
            let detail = if self.catalog.state.favorites.contains(&entry.id) {
                format!("★  {alias}")
            } else {
                alias.clone()
            };
            list = list.child(
                div()
                    .id(("result", i))
                    .role(Role::ListBoxOption)
                    .aria_label(format!("{}, {}", entry.name, entry.kind.label()))
                    .aria_selected(i == self.selected)
                    .h(px(52.))
                    .flex_shrink_0()
                    .mx_1()
                    .my(px(2.))
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_4()
                    .rounded_lg()
                    .cursor_pointer()
                    .when(i == self.selected, |d| {
                        d.bg(rgb(p.get("selection")))
                            .border_l_2()
                            .border_color(rgb(p.get("accent")))
                    })
                    .hover(|d| d.bg(rgb(p.get("surface"))))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected = i;
                        if let Some(entry) = this.results.get(i).cloned() {
                            this.run_entry(entry, window, cx);
                        }
                    }))
                    .child(icon)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_3()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .max_w(px(350.))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(entry.name.clone()),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_sm()
                                    .text_color(rgb(p.get("muted")))
                                    .child(detail),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(rgb(p.get("muted")))
                            .child(entry.kind.label()),
                    ),
            );
        }
        shell
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if this.actions.is_some() {
                        this.close_actions(window, cx);
                    }
                }),
            )
            .child(
                div()
                    .h(px(82.))
                    .flex_shrink_0()
                    .px_6()
                    .flex()
                    .items_center()
                    .gap_4()
                    .border_b_1()
                    .border_color(rgb(p.get("border")))
                    .child(div().text_3xl().text_color(rgb(p.get("accent"))).child("◈"))
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.search)
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false)
                                .text_size(px(20.)),
                        ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(p.get("selection")))
                            .text_color(rgb(p.get("muted")))
                            .child("esc"),
                    ),
            )
            .child(list)
            .child(
                div()
                    .h(px(48.))
                    .flex_shrink_0()
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_2()
                    .bg(rgb(p.get("surface")))
                    .border_t_1()
                    .border_color(rgb(p.get("border")))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(p.get("muted")))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .max_w(px(360.))
                            .child(
                                self.message
                                    .as_ref()
                                    .map(|(s, _)| s.clone())
                                    .unwrap_or_else(|| "◈  arch cast".into()),
                            ),
                    )
                    .child(
                        Button::new("settings")
                            .ghost()
                            .small()
                            .label("Settings")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_settings(window, cx)),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div().text_sm().child(
                            if self
                                .results
                                .get(self.selected)
                                .is_some_and(|e| e.kind == EntryKind::Calculator)
                            {
                                "Copy ↵"
                            } else {
                                "Open ↵"
                            },
                        ),
                    )
                    .child(
                        Button::new("actions")
                            .ghost()
                            .small()
                            .label("Actions  Ctrl K")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.toggle_actions(window, cx)),
                            ),
                    ),
            )
            .when(self.actions.is_some(), |d| d.child(self.render_actions(cx)))
            .when(self.alias.is_some(), |d| d.child(self.render_alias(cx)))
    }
}
