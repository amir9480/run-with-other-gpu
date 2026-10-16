// GUI subsystem: no console window, so the background `--run` helper stays invisible.
#![windows_subsystem = "windows"]

#[cfg(not(windows))]
compile_error!("run-with-other-gpu only supports Windows");

mod gpu;
mod launch;
mod menu;
mod prefs;
mod state;
mod ui;
mod util;
mod vcredist;

use std::ffi::OsString;

const USAGE: &str = "\
Usage:
  run-with-other-gpu.exe                       Show the install/remove dialog
  run-with-other-gpu.exe --install             Install the Explorer context menu
  run-with-other-gpu.exe --remove              Remove the Explorer context menu (alias: --uninstall)
  run-with-other-gpu.exe --run <role> <exe>    Run <exe> on the GPU <role>
                                               (power-saving | high-performance)";

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();

    // Undo GPU preferences left behind by a helper that crashed or was killed.
    let _ = state::recover_stale();

    let code = match args.first().and_then(|a| a.to_str()) {
        None => {
            ui::run();
            0
        }
        Some("--install") => cli_result(menu::install().map(|gpus| {
            let mut msg = String::from("Context menu installed.");
            for g in gpus {
                msg.push_str(&format!("\n  {}", g.menu_label()));
            }
            msg
        })),
        Some("--remove") | Some("--uninstall") => cli_result(menu::remove()),
        Some("--run") => launch::run(&args[1..]),
        Some("--help") | Some("-h") | Some("/?") => {
            util::attach_console();
            println!("{USAGE}");
            0
        }
        Some(other) => {
            util::attach_console();
            eprintln!("Unknown argument: {other}\n\n{USAGE}");
            2
        }
    };
    std::process::exit(code);
}

fn cli_result(result: anyhow::Result<String>) -> i32 {
    util::attach_console();
    match result {
        Ok(msg) => {
            println!("{msg}");
            0
        }
        Err(e) => {
            eprintln!("Error: {e:#}");
            1
        }
    }
}
