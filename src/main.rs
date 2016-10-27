#[macro_use]
extern crate log;
extern crate log4rs;
extern crate irc;
extern crate time;
extern crate yaml_rust;

mod checker;
mod config;
mod chat;

use std::default::Default;
use std::io::{Result, Error, ErrorKind};
use std::path::Path;

use config::HammerConfig;
use chat::Chat;

fn main() {
    init_logger().expect("An error occured while initializing the logging system. If you don't need logging, you can just remove the 'logging.yml' file.");

    let app_config = load_config().expect("An error occured while loading the application's configuration.");

    let mut chat = Chat::new(&app_config);
    chat.run();
}

fn init_logger() -> std::result::Result<(), log4rs::Error> {
    let conf_file_name = "logging.yml";
    if Path::new(conf_file_name).exists() {
        log4rs::init_file(&conf_file_name, Default::default())
    }
    else {
        // Nothing to do here
        Ok(())
    }
}

fn load_config() -> Result<HammerConfig> {
    let mut result = HammerConfig::new();
    try!(result.fill_from_file("config.yml"));

    // Load a developper configuration if there is one
    let dev_config_name = Path::new("config-dev.yml");
    if dev_config_name.exists() {
        try!(result.fill_from_file(dev_config_name));
    }

    if !result.validate() {
        return Err(Error::new(ErrorKind::InvalidData, "The configuration is invalid! I'm out."));
    }

    Ok(result)
}
