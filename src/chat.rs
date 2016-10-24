extern crate irc;

use std::collections::HashMap;

use irc::client::prelude::*;
use irc::client::data::command::CapSubCommand;

use checker::Checker;
use config::HammerConfig;

const CAP_MEMBERSHIP : &'static str = "twitch.tv/membership";
const CAP_COMMANDS : &'static str = "twitch.tv/commands";
const CAP_TAGS : &'static str = "twitch.tv/tags";

pub struct Chat {
    server: IrcServer,
    channel: String, 
    checker: Checker,
    cap_membership_enabled: bool,
    cap_commands_enabled: bool,
    cap_tags_enabled: bool,
    all_users: HashMap<String, ChatUser>,
}

struct ChatUser {
    name: String,
    is_mod: bool,
    is_regular: bool,
}

impl Chat {
    pub fn new(conf : &HammerConfig) -> Chat {
        if let Some(ref channel) = conf.channel {
            let streamer_name = channel.to_lowercase();
            
            let mut result = Chat {
                server: IrcServer::from_config(conf.to_irc_config()).unwrap(),
                channel: format!("#{}", streamer_name),
                checker: Checker::new(),
                cap_membership_enabled: false,
                cap_commands_enabled: false,
                cap_tags_enabled: false,
                all_users: HashMap::new(),
            };

            result.all_users.insert(streamer_name.clone(), ChatUser {
                name: streamer_name,
                is_mod: true,
                is_regular: true,
            });

            result
        }
        else {
            panic!("The configuration has not been correctly initialized");
        }

    }

    pub fn run(&mut self) {
        info!("Connecting to IRC for channel {} ...", self.channel);
        self.server.identify().unwrap();
        info!("Connected!");

        // activate Twitch capabilities
        // https://github.com/justintv/Twitch-API/blob/master/IRC.md
        self.server.send_cap_req(&[
            Capability::Custom(CAP_MEMBERSHIP), 
            Capability::Custom(CAP_COMMANDS),
            Capability::Custom(CAP_TAGS)]).expect("Could not send capability requests");

        loop {
            if let Some(message) = self.read_next_message() {
                if !self.process_message(message) {
                    break;
                }
            }
            else {
                // No more messages; exit
                break;
            }
        }

        info!("Disconnected from channel {}", self.channel);
    }

    fn read_next_message(&self) -> Option<Message> {
        for msg in self.server.iter() {
            match msg {
                Ok(result) => {
                    debug!("Message received : {}", result);
                    return Some(result);
                },
                Err(err) => debug!("Error while reading a message: {}", err), 
            }
        };

        return None;
    }

    fn process_message(&mut self, message: Message) -> bool {
        match message.command {
            Command::PRIVMSG(ref nickname, ref msg) => {
                self.user_ensure_exists(nickname);

                let user_is_known: bool;
                {
                    let ref user = self.all_users[nickname];
                    user_is_known = user.is_mod || user.is_regular;
                }

                if !user_is_known && self.checker.check(msg.trim()) {
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
            },
            Command::MODE(channel, mode, nickname) => {
                if channel == self.channel {
                    if let Some(ref nickname_str) = nickname {
                        match mode.as_ref() {
                            "+o" => self.user_set_is_mod(nickname_str, true),
                            "-o" => self.user_set_is_mod(nickname_str, false),
                            _ => debug!("Unhandled MODE change '{}' on user '{}'.", mode, nickname_str),
                        }
                    }
                }
            }
            _ => debug!("Unhandled command {:?}", message.command),
        }

        true
    }

    fn send(&self, msg: &str) {
        if let Err(error) = self.server.send_privmsg(self.channel.as_str(), msg) {
            error!("Could not send a message on {}!", self.channel);
            debug!(" - Message was '{}'", msg);
            debug!(" - Error was {}", error);
        }
    }

    fn parse_user_name(user_full_name: &str) -> Option<&str> {
        if let Some(pos) = user_full_name.find('!') {
            Some(&user_full_name[..pos])
        }
        else {
            info!("Invalid user descriptor, could not parse. '{}'", user_full_name);
            None
        }
    }

    fn user_set_is_mod(&mut self, user_full_name: &str, is_mod: bool) {
        if let Some(nickname) = Chat::parse_user_name(user_full_name) {
            self.user_ensure_exists(nickname);
            if let Some(user) = self.all_users.get_mut(nickname) {
                user.is_mod = is_mod;
            }
        }
    }

    fn user_ensure_exists(&mut self, nickname: &str) -> bool {
        if self.all_users.contains_key(nickname) {
            true
        }
        else {
            let owned_nickname = nickname.to_owned();
            // Create a new user
            let new_user = ChatUser {
                name: owned_nickname.clone(),
                is_mod: false,
                is_regular: false,
            };

            // Add it to the list
            self.all_users.insert(owned_nickname, new_user);
            false
        }
    }
}