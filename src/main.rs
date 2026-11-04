// SPDX-License-Identifier: GPL-3.0-only

mod app;
mod i18n;

fn main() -> cosmic::iced::Result {
    init_logging();
    i18n::init(&i18n_embed::DesktopLanguageRequester::requested_languages());

    let settings = cosmic::app::Settings::default()
        .no_main_window(true)
        .exit_on_close(false);
    cosmic::app::run::<app::App>(settings, ())
}

fn init_logging() {
    use std::io::IsTerminal;
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,cosmic_sticky=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
}
