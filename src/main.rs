// Release builds use the GUI subsystem so starting at logon does not flash a
// console window; diagnostics attach to the parent terminal when there is one.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use window_placer::app::{self, Command};
use window_placer::win;

fn main() {
    win::attach_parent_console();
    let code = match app::parse_args(std::env::args().skip(1)) {
        Ok(Command::Help) => {
            print!("{}", app::USAGE);
            0
        }
        Ok(Command::Run(opts)) => app::run(opts),
        Err(msg) => {
            eprintln!("window-placer: {msg}\n\n{}", app::USAGE);
            2
        }
    };
    std::process::exit(code);
}
