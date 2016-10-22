#[macro_use]
extern crate log;
extern crate log4rs;
extern crate irc;
extern crate yaml_rust;

mod checker;
mod config;

use std::default::Default;
use std::io::Result;
use std::path::Path;

use irc::client::prelude::*;

use config::HammerConfig;

fn main() {
    init_logger().expect("An error occured while initializing the logging system. If you don't need logging, you can just remove the 'logging.yml' file.");

    let app_config = load_config().expect("An error occured while loading the application's configuration.");

    let irc_config = app_config.to_irc_config();

    info!("Connecting to IRC...");
    let server = IrcServer::from_config(irc_config).unwrap();
    server.identify().unwrap();
    info!("Connected!");

    let my_checker = checker::Checker::new();

    for message in server.iter() {
        let message = message.unwrap();
        debug!("Message received : {}", message);
        match message.command {
            Command::JOIN(ref target, _, _) => server.send_privmsg(target, "hi").unwrap(),
            Command::PRIVMSG(ref target, ref msg) => {
                if my_checker.check(msg.trim()) {
                    server.send_privmsg(target, "DansGame").unwrap();
                }
                else {
                    server.send_privmsg(target, "FrankerZ").unwrap();
                }
            },
            _ => (),
        }
    }
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

    Ok(result)
}