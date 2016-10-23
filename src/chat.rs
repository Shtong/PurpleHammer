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
    name: String, 
    checker: Checker,
    cap_membership_enabled: bool,
    cap_commands_enabled: bool,
    cap_tags_enabled: bool,
    all_users: Vec<ChatUser>, // Chat owner (streamer) is always at index 0!
    user_index_names: HashMap<String, usize>,
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
                name: format!("#{}", streamer_name),
                checker: Checker::new(),
                cap_membership_enabled: false,
                cap_commands_enabled: false,
                cap_tags_enabled: false,
                all_users: Vec::new(),
                user_index_names: HashMap::new(),
            };

            result.user_index_names.insert(streamer_name.clone(), 0);
            result.all_users.push(ChatUser {
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
                Command::PRIVMSG(ref nickname, ref msg) => {
                    let ref user = self.all_users[self.user_get_or_create(nickname)];

                    if !user.is_mod && !user.is_regular && self.checker.check(msg.trim()) {
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
                    if channel == self.name {
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
        if let Some(user_name) = Chat::parse_user_name(user_full_name) {
            let user_pos = self.user_get_or_create(user_name);
            self.all_users[user_pos].is_mod = is_mod;
        }
    }

    fn user_get_or_create(&mut self, user_name: &str) -> usize {
        if let Some(pos) = self.user_index_names.get(user_name) {
            *pos
        }
        else {
            let owned_name = user_name.to_owned();
            // Create a new user
            let new_user = ChatUser {
                name: owned_name,
                is_mod: false,
                is_regular: false,
            };

            // Add it to the list
            let pos = self.all_users.len();
            self.user_index_names.insert(new_user.name.clone(), pos);
            self.all_users.push(new_user);
            pos
        }
    }
}