#![windows_subsystem = "windows"]

mod config;
mod luna;
mod photo;
mod ui;

fn main() {
    config::init();
    std::process::exit(ui::run());
}
