extern crate irc;

use std::collections::HashMap;

use irc::client::prelude::*;
use irc::client::data::command::CapSubCommand;
use irc::client::data::message::Tag;
use time::{Tm, now_utc};

use checker::Checker;
use config::HammerConfig;

const CAP_MEMBERSHIP : &'static str = "twitch.tv/membership";
const CAP_COMMANDS : &'static str = "twitch.tv/commands";
const CAP_TAGS : &'static str = "twitch.tv/tags";

#[derive(Debug)]
struct ChatUser {
    nickname: String,
    is_mod: bool,
    is_paying: bool,
    auto_ban_date: Option<Tm>,
}

impl ChatUser {
    fn new(nickname: String) -> ChatUser {
        ChatUser {
            nickname: nickname,
            is_mod: false,
            is_paying: false,
            auto_ban_date: None,
        }
    }
}

pub struct Chat {
    server: IrcServer,
    channel: String, 
    checker: Checker,
    cap_membership_enabled: bool,
    cap_commands_enabled: bool,
    cap_tags_enabled: bool,
    all_users: HashMap<String, ChatUser>,
    ban_mode_enabled: bool,
    my_nickname: String,
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
                ban_mode_enabled: false,
                all_users: HashMap::new(),
                my_nickname: conf.username.clone().unwrap(),
            };

            let mut streamer = ChatUser::new(streamer_name.clone());
            streamer.is_mod = true;
            result.all_users.insert(streamer_name, streamer);

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

        info!("Disconnected from server");
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
        let start_time = now_utc();
        match message.command {
            Command::PRIVMSG(ref nickname, ref msg) => {
                if nickname != self.my_nickname.as_str() { // Ignore messages sent by me
                    self.user_ensure_exists(nickname);
                    let user_is_protected;
                    let user_is_mod;
                    {
                        if let Some(ref tags) = message.tags {
                            self.parse_tags(nickname, tags);
                        }
                        let ref user = self.all_users[nickname];
                        user_is_mod = user.is_mod;


                        user_is_protected = user_is_mod || // Don't ban mods
                                            user.is_paying || // Don't ban paying users (subs, turbo etc..), they're not bots
                                            user.auto_ban_date.is_some(); // Don't reban unbanned users
                    }

                    debug!("Tags: {:?}", message.tags);

                    if msg == ":hammer on" {
                        if user_is_mod {
                            self.ban_mode_enabled = true;
                            self.send("⚠️ ATTENTION : Hammer mode has been enabled. Please refrain from sending messages that could look like what a bot would say!");
                        }
                    }
                    else if msg == ":hammer off" {
                        if user_is_mod {
                            self.ban_mode_enabled = false;
                            self.send("Hammer mode has been disabled. I'll stop banning now!");
                        }
                    }
                    else if self.ban_mode_enabled {
                        if !user_is_protected && self.checker.check(msg.trim()) {
                            // rip
                            self.send(&format!("/ban {}", nickname));
                            self.all_users.get_mut(nickname).unwrap().auto_ban_date = Some(now_utc());
                        }
                    }
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
                        self.user_ensure_exists(nickname_str);
                        match mode.as_ref() {
                            "+o" => self.all_users.get_mut(nickname_str).unwrap().is_mod = true,
                            "-o" => self.all_users.get_mut(nickname_str).unwrap().is_mod = false,
                            _ => debug!("Unhandled MODE change '{}' on user '{}'.", mode, nickname_str),
                        }
                    }
                }
            }
            Command::NOTICE(_, message) => {
                if message == "Login authentication failed" {
                    // Whops
                    error!("The remote server rejected the OAuth token. Make sure it is correct in your configuration file!");
                    // We could exit here, but we'll let the connection close by itself
                }
            }
            _ => debug!("Unhandled command {:?}", message.command),
        }

        debug!("Message processsed in {}ms", (now_utc() - start_time).num_milliseconds());

        true
    }

    fn send(&self, msg: &str) {
        if let Err(error) = self.server.send_privmsg(self.channel.as_str(), msg) {
            error!("Could not send a message on {}!", self.channel);
            debug!(" - Message was '{}'", msg);
            debug!(" - Error was {}", error);
        }
    }

    // fn parse_user_name(user_full_name: &str) -> Option<&str> {
    //     if let Some(pos) = user_full_name.find('!') {
    //         Some(&user_full_name[..pos])
    //     }
    //     else {
    //         info!("Invalid user descriptor, could not parse. '{}'", user_full_name);
    //         None
    //     }
    // }

    // fn user_set_is_mod(&mut self, nickname: &str, is_mod: bool) {
    //     if let Some(user) = self.all_users.get_mut(nickname) {
    //         user.is_mod = is_mod;
    //     }
    // }

    fn user_ensure_exists(&mut self, nickname: &str) -> bool {
        if self.all_users.contains_key(nickname) {
            true
        }
        else {
            let owned_nickname = nickname.to_owned();
            // Add a new user to the list
            self.all_users.insert(owned_nickname.clone(), ChatUser::new(owned_nickname));
            false
        }
    }

    fn parse_tags(&mut self, nickname: &str, tags: &Vec<Tag>) {
        let mut user = self.all_users.get_mut(nickname).unwrap();
        for tag in tags {
            let &Tag(ref key, ref val_opt) = tag;
            if let &Some(ref val) = val_opt {
                match key.as_str() {
                    "subscriber" => if val == "1" { user.is_paying = true },
                    "turbo" => if val == "1" { user.is_paying = true },
                    "user-type" => if val.len() > 0 { user.is_mod = true },
                    &_ => {}
                }
            }
        }
    }
}

// #[cfg(test)]
// mod test {
//     use super::*;

//     #[test]
//     fn parse_user_name_correct() {
//         assert_eq!(Some("MyUser"), Chat::parse_user_name("MyUser!myuser@tmi.twitch.tv"));
//     }

//     #[test]
//     fn parse_user_name_incorrect() {
//         assert_eq!(None, Chat::parse_user_name("u wot?"));
//     }
// }