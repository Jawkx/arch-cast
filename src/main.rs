mod ui;

use anyhow::{Context as _, Result, bail};
use arch_cast::{
    catalog::Catalog,
    ipc::{self, Command, Request, Server},
    paths::Paths,
    platform,
};
use clap::Parser;
use gpui::{prelude::*, *};
use std::time::Duration;

#[derive(Parser)]
#[command(
    version,
    about = "A keyboard-first native GPUI launcher for Arch Linux and Hyprland"
)]
struct Args {
    #[arg(value_enum, default_value = "toggle")]
    command: Command,
    #[arg(default_value = "")]
    argument: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("arch-cast: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args = Args::parse();
    let paths = Paths::from_env();
    match args.command {
        Command::Doctor => return platform::doctor(&paths),
        Command::List => {
            let entries = Catalog::new(paths).search(&args.argument);
            let rows: Vec<_> = entries
                .iter()
                .map(|e| serde_json::json!({"id":e.id,"name":e.name,"kind":e.kind}))
                .collect();
            println!("{}", serde_json::to_string_pretty(&rows)?);
            return Ok(());
        }
        Command::Install => return arch_cast::install::install(&paths),
        Command::Uninstall => return arch_cast::install::uninstall(&paths),
        Command::Run if args.argument.is_empty() => {
            bail!("run requires an entry ID; use 'arch-cast list' to find one")
        }
        _ => {}
    }
    let request = Request {
        command: args.command,
        argument: args.argument,
    };
    if ipc::send(&paths.runtime, &request).is_ok() {
        return Ok(());
    }
    if matches!(request.command, Command::Hide | Command::Quit) {
        return Ok(());
    }
    if request.command == Command::Ping {
        bail!("resident is not running");
    }
    if request.command != Command::Daemon {
        let mut child = platform::start_resident(&paths)?;
        for _ in 0..100 {
            std::thread::sleep(Duration::from_millis(100));
            if ipc::send(&paths.runtime, &request).is_ok() {
                return Ok(());
            }
            if child.try_wait()?.is_some() {
                break;
            }
        }
        bail!(
            "could not start resident; see {}",
            paths.state.join("launcher.log").display()
        );
    }
    let Some(server) = Server::bind(&paths.runtime)? else {
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(100));
            if ipc::send(&paths.runtime, &Request::new(Command::Ping)).is_ok() {
                return Ok(());
            }
        }
        bail!("another resident holds the lock but does not respond");
    };
    let requests = server.start()?;
    gpui::application()
        .with_assets(gpui::assets::Assets)
        .with_quit_mode(QuitMode::Explicit)
        .run(move |cx| {
            gpui::init(cx);
            let mut resident = Resident {
                paths,
                window: None,
                view: None,
            };
            cx.spawn(async move |cx| {
                while let Ok(request) = requests.recv().await {
                    cx.update(|cx| resident.dispatch(request, cx));
                }
            })
            .detach();
        });
    drop(server);
    Ok(())
}

struct Resident {
    paths: Paths,
    window: Option<AnyWindowHandle>,
    view: Option<Entity<ui::Launcher>>,
}
impl Resident {
    fn visible(&self, cx: &App) -> bool {
        self.window.is_some_and(|window| {
            cx.windows()
                .iter()
                .any(|w| w.window_id() == window.window_id())
        })
    }
    fn hide(&mut self, cx: &mut App) {
        if let Some(window) = self.window.take() {
            let _ = window.update(cx, |_, window, _| window.remove_window());
        }
        self.view = None;
    }
    fn show(&mut self, cx: &mut App) -> Result<()> {
        if self.visible(cx) {
            if let Some(window) = self.window {
                window.update(cx, |_, window, _| window.activate_window())?;
            }
            platform::focus_launcher();
            return Ok(());
        }
        let config = Catalog::new(self.paths.clone()).config;
        let bounds = Bounds::centered(None, size(px(config.width as f32), px(584.)), cx);
        let paths = self.paths.clone();
        let (window, view) = gpui::open_window(
            WindowOptions {
                app_id: Some("arch-cast".into()),
                titlebar: None,
                window_background: WindowBackgroundAppearance::Transparent,
                window_decorations: Some(WindowDecorations::Client),
                is_resizable: false,
                is_minimizable: false,
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(640.), px(584.))),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| ui::Launcher::new(Catalog::new(paths), window, cx)),
        )
        .context("open GPUI launcher window")?;
        self.window = Some(window);
        self.view = Some(view);
        Ok(())
    }
    fn dispatch(&mut self, request: Request, cx: &mut App) {
        let result = match request.command {
            Command::Toggle if self.visible(cx) => {
                self.hide(cx);
                Ok(())
            }
            Command::Toggle | Command::Show => self.show(cx),
            Command::Hide => {
                self.hide(cx);
                Ok(())
            }
            Command::Quit => {
                cx.quit();
                Ok(())
            }
            Command::Run => self.show(cx).and_then(|()| {
                let view = self.view.as_ref().unwrap().clone();
                self.window.unwrap().update(cx, |_, window, cx| {
                    view.update(cx, |view, cx| view.run_id(&request.argument, window, cx))
                })
            }),
            _ => Ok(()),
        };
        if let Err(error) = result {
            eprintln!("arch-cast: {error:#}");
        }
    }
}
