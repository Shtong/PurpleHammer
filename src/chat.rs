extern crate irc;

use irc::client::prelude::*;
use irc::client::data::command::CapSubCommand;

use checker::Checker;
use config::HammerConfig;

const CAP_MEMBERSHIP : &'static str = "twitch.tv/membership";
const CAP_COMMANDS : &'static str = "twitch.tv/commands";
const CAP_TAGS : &'static str = "twitch.tv/tags";

pub struct Chat {
    server: IrcServer,
    name: String, 
    checker: Checker,
    cap_membership_enabled: bool,
    cap_commands_enabled: bool,
    cap_tags_enabled: bool,
}

impl Chat {
    pub fn new(conf : &HammerConfig) -> Chat {
        Chat {
            server: IrcServer::from_config(conf.to_irc_config()).unwrap(),
            name: conf.get_irc_channel().unwrap(),
            checker: Checker::new(),
            cap_membership_enabled: false,
            cap_commands_enabled: false,
            cap_tags_enabled: false,
        }
    }

    pub fn run(&mut self) {
        info!("Connecting to IRC for channel {} ...", self.name);
        self.server.identify().unwrap();
        info!("Connected!");

        // activate Twitch capabilities
        // https://github.com/justintv/Twitch-API/blob/master/IRC.md
        self.server.send_cap_req(&[
            Capability::Custom(CAP_MEMBERSHIP), 
            Capability::Custom(CAP_COMMANDS),
            Capability::Custom(CAP_TAGS)]).expect("Could not send capability requests");

        for message in self.server.iter() {
            let message = message.unwrap();
            debug!("Message received : {}", message);
            match message.command {
                Command::JOIN(ref target, _, _) => {
                    if target == self.name.as_str() {
                        self.server.send_privmsg(target, "Hi").unwrap();
                    }
                    else {
                        warn!("I joined an unexpected channel '{}'!", target);
                    }
                },
                Command::PRIVMSG(_, ref msg) => {
                    if self.checker.check(msg.trim()) {
                        self.send("DansGame");
                    }
                    else {
                        self.send("FrankerZ");
                    }
                },
                Command::CAP(_, sub_command, _, param) => {
                    match sub_command {
                        CapSubCommand::ACK => {
                            if let Some(param_str) = param {
                                for one_param_str in param_str.split_whitespace() {
                                    match one_param_str {
                                        CAP_COMMANDS => self.cap_commands_enabled = true,
                                        CAP_MEMBERSHIP => self.cap_membership_enabled = true,
                                        CAP_TAGS => self.cap_tags_enabled = true,
                                        &_ => debug!("Capability {} acknowledged", param_str),
                                    }
                                }
                            }
                            else {
                                warn!("The server validated a capability, but I don't know which one?!?");
                            }
                        }
                        _ => {},
                    }
                }
                _ => (),
            }
        }

        info!("Disconnected from channel {}", self.name);
    }

    fn send(&self, msg: &str) {
        if let Err(error) = self.server.send_privmsg(self.name.as_str(), msg) {
            error!("Could not send a message on {}!", self.name);
            debug!(" - Message was '{}'", msg);
            debug!(" - Error was {}", error);
        }
    }
}