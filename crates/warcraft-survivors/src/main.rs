//! The `warcraft-survivors` launcher: the build stamp, handed to the survivors mode.

// The player build is a desktop app: no console window behind the game on Windows. A dev build
// keeps it, for the log.
#![cfg_attr(all(windows, not(feature = "dev")), windows_subsystem = "windows")]

use benilla_app::BuildId;

fn main() -> benilla_app::AppExit {
    benilla_app::run_survivors(BuildId {
        version: env!("CARGO_PKG_VERSION"),
        describe: env!("BENILLA_GIT_DESCRIBE"),
        sha: env!("BENILLA_GIT_SHA"),
        short: env!("BENILLA_GIT_SHORT"),
        date: env!("BENILLA_GIT_DATE"),
        profile: env!("BENILLA_PROFILE"),
        project_dir: env!("BENILLA_PROJECT_DIR"),
        ..Default::default()
    })
}
