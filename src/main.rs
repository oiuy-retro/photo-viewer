#![windows_subsystem = "windows"]

mod luna;
mod photo;
mod ui;

fn main() {
    std::process::exit(ui::run());
}
