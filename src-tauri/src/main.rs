#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let mut args = std::env::args_os().skip(1);
    if args.next().is_some_and(|v| v == "--check-pdf") {
        let Some(output) = args.next() else {
            std::process::exit(2);
        };
        std::process::exit(
            if tell_you_why_lib::check_pdf(std::path::Path::new(&output)).is_ok() {
                0
            } else {
                1
            },
        );
    }
    tell_you_why_lib::run();
}
