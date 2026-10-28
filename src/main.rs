mod backend;
mod models;
mod ui;

use adw::prelude::*;

const APP_ID: &str = "io.github.shridhargsabat.DriverManager";

fn main() {
    env_logger::init();

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(ui::build_ui);
    app.run();
}
