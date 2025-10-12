use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use glide_wm::actor::channel;
use glide_wm::actor::layout::LayoutManager;
use glide_wm::actor::mouse::Mouse;
use glide_wm::actor::notification_center::NotificationCenter;
use glide_wm::actor::reactor::{self, Reactor};
use glide_wm::actor::status::Status;
use glide_wm::actor::wm_controller::{self, WmController};
use glide_wm::config::{Config, config_file, restore_file};
use glide_wm::log;
use glide_wm::sys::executor::Executor;
use glide_wm::sys::screen::CoordinateConverter;
use objc2::MainThreadMarker;
use tokio::join;

#[derive(Parser)]
struct Cli {
    /// Only run the window manager on the current space.
    #[arg(long)]
    one: bool,

    /// Disable new spaces by default.
    ///
    /// Ignored if --one is used.
    #[arg(long)]
    default_disable: bool,

    /// Disable animations.
    #[arg(long)]
    no_animate: bool,

    /// Check whether the restore file can be loaded without actually starting
    /// the window manager.
    #[arg(long)]
    validate: bool,

    /// Restore the configuration saved with the save_and_exit command. This is
    /// only useful within the same session.
    #[arg(long)]
    restore: bool,

    /// Record reactor events to the specified file path. Overwrites the file if
    /// exists.
    #[arg(long)]
    record: Option<PathBuf>,
}

fn main() {
    let opt: Cli = Parser::parse();

    if std::env::var_os("RUST_BACKTRACE").is_none() {
        // SAFETY: We are single threaded at this point.
        unsafe { std::env::set_var("RUST_BACKTRACE", "1") };
    }
    log::init_logging();
    install_panic_hook();

    let mut config = if config_file().exists() {
        Config::read(&config_file()).unwrap()
    } else {
        Config::default()
    };
    config.settings.animate &= !opt.no_animate;
    config.settings.default_disable |= opt.default_disable;
    let config = Arc::new(config);

    if opt.validate {
        LayoutManager::load(restore_file()).unwrap();
        return;
    }

    let layout = if opt.restore {
        LayoutManager::load(restore_file()).unwrap()
    } else {
        LayoutManager::new()
    };
    let (mouse_tx, mouse_rx) = channel();
    let (status_tx, status_rx) = channel();
    let mtm = MainThreadMarker::new().unwrap();

    // Create group indicators actor
    let (group_indicators_tx, group_indicators_rx) = glide_wm::actor::channel();

    let events_tx = Reactor::spawn(
        config.clone(),
        layout,
        reactor::Record::new(opt.record.as_deref()),
        mouse_tx.clone(),
        status_tx.clone(),
        group_indicators_tx,
    );
    let wm_config = wm_controller::Config {
        one_space: opt.one,
        restore_file: restore_file(),
        config: config.clone(),
    };
    let (wm_controller, wm_controller_sender) =
        WmController::new(wm_config, events_tx.clone(), mouse_tx.clone(), status_tx);
    let notification_center = NotificationCenter::new(wm_controller_sender);
    let mouse = Mouse::new(config.clone(), events_tx, mouse_rx);
    let status = Status::new(config.clone(), status_rx, mtm);

    let group_indicators = glide_wm::actor::group_indicators::GroupIndicators::new(
        config.clone(),
        group_indicators_rx,
        mtm,
        CoordinateConverter::default(),
    );

    Executor::run_main(mtm, async move {
        join!(
            wm_controller.run(),
            notification_center.watch_for_notifications(),
            mouse.run(),
            status.run(),
            group_indicators.run(),
        );
    });
}

#[cfg(panic = "unwind")]
fn install_panic_hook() {
    // Abort on panic instead of propagating panics to the main thread.
    // See Cargo.toml for why we don't use panic=abort everywhere.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        original_hook(info);
        std::process::abort();
    }));
}

#[cfg(not(panic = "unwind"))]
fn install_panic_hook() {}
