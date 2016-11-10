extern crate irc;

use std::collections::HashMap;
use std::str::FromStr;

use irc::client::prelude::*;
use irc::client::data::command::CapSubCommand;
use irc::client::data::message::Tag;
use time::{Tm, now_utc};

use checker::Checker;
use config::HammerConfig;

const CAP_MEMBERSHIP : &'static str = "twitch.tv/membership";
const CAP_COMMANDS : &'static str = "twitch.tv/commands";
const CAP_TAGS : &'static str = "twitch.tv/tags";

enum ChatMessage {
    /// Incoming text message (author nickname, text, tags)
    Message(String, String, MessageTagData),
    // A user joined the chat (nickname)
    Join(String),
    /// A user left the chat (nickname)
    Leave(String),
    /// The channel was cleared
    Clear,
    /// A user was timed out (nickname, duration, reason)
    Timeout(String, u16, Option<String>),
    /// A user was banned (nickname, reason)
    Ban(String, Option<String>),
    /// A user was unbanned (nickname)
    Unban(String),
    /// Someone gained or lost operator status (nickname, is_op)
    Operator(String, bool),
    /// Room state
    RoomState(RoomStateTags),
    /// Server capabilities acknowledgement
    Capability(Vec<String>),
    /// Invalid auth token notification
    InvalidAuthToken,
}

enum TwitchUserType {
    None,
    Mod,
    GlobalMod,
    Admin,
    Staff,
    Other(String),
}

impl Default for TwitchUserType {
    fn default() -> TwitchUserType {
        TwitchUserType::None
    }
}

impl From<String> for TwitchUserType {
    fn from(input: String) -> TwitchUserType {
        match input.as_str() {
            "" => TwitchUserType::None,
            "mod" => TwitchUserType::Mod,
            "global_mod" => TwitchUserType::GlobalMod,
            "admin" => TwitchUserType::Admin,
            "staff" => TwitchUserType::Staff,
            _ => TwitchUserType::Other(input),
        }
    }
}

#[derive(Default)]
struct MessageTagData {
    //badges: Vec<TwitchBadge>, // TODO
    color: Option<String>,
    display_name: Option<String>,
    //emotes: // TODO
    id: Option<String>, // TODO: Store in a UUID/GUID type
    is_mod: Option<bool>,
    is_subscriber: Option<bool>,
    is_turbo: Option<bool>,
    room_id: Option<u32>,
    user_id: Option<u32>,
    user_type: Option<TwitchUserType>,
}

impl MessageTagData {
    fn from_tags(tags: Vec<Tag>) -> Result<MessageTagData, String> {
        let mut result = MessageTagData {
            ..Default::default()
        };

        for tag in tags {
            let Tag(key, val_opt) = tag;
            if let Some(val) = val_opt {
                match key.as_str() {
                    "badges" => { /* SKIP */ },
                    "color" => result.color = Some(val),
                    "display-name" => result.color = Some(val),
                    "emotes" => { /* SKIP */ },
                    "id" => result.id = Some(val),
                    "mod" => result.is_mod = Some(val == "1"),
                    "subscriber" => result.is_subscriber = Some(val == "1"),
                    "turbo" => result.is_turbo = Some(val == "1"),
                    "room-id" => {
                        if let Ok(parsed) = u32::from_str(val.as_str()) {
                            result.room_id = Some(parsed);
                        }
                        else {
                            return Err(format!("Could not parse the room id '{}'", val));
                        }
                    },
                    "user-id" => {
                        if let Ok(parsed) = u32::from_str(val.as_str()) {
                            result.user_id = Some(parsed);
                        }
                        else {
                            return Err(format!("Could not parse the user id '{}'", val));
                        }
                    },
                    "user-type" => result.user_type = Some(TwitchUserType::from(val)),
                    &_ => debug!("Unexpected message tag: {}={}", key, val),
                }
            }
        };

        Ok(result)
    }
}

struct RoomStateTags {
    language: Option<String>,
    r9k: Option<bool>,
    subs_only: Option<bool>,
    slow: Option<bool>,
}

impl RoomStateTags {
    fn from_tags_list(tags: Vec<Tag>) -> RoomStateTags {
        let mut result = RoomStateTags {
            language: None,
            r9k: None,
            subs_only: None,
            slow: None,
        };

        for tag in tags {
            let Tag(key, val_opt) = tag;
            if let Some(val) = val_opt {
                match key.as_str() {
                    "language" => result.language = Some(val),
                    "r9k" => result.r9k = Some(val.as_str() == "1"),
                    "subs-only" => result.subs_only = Some(val.as_str() == "1"),
                    "slow" => result.slow = Some(val.as_str() == "1"),
                    &_ => debug!("Unexpected room state tag: {}={}", key, val),
                }
            }
        }

        result
    }
}

#[derive(Debug)]
struct ChatUser {
    nickname: String,
    display_name: String,
    is_mod: bool,
    is_paying: bool,
    auto_ban_date: Option<Tm>,
}

impl ChatUser {
    fn new(nickname: String) -> ChatUser {
        ChatUser {
            nickname: nickname.clone(),
            display_name: nickname,
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

    /// Waits for the next message from the server and returns it.
    fn read_next_message(&self) -> Option<ChatMessage> {
        for msg in self.server.iter() {
            match msg {
                Ok(result) => {
                    debug!("Message received : {}", result);
                    let result = Chat::parse_message(result);
                    if result.is_some() {
                        return result;
                    }
                    // if result is none, we skip that message and wait for the next one
                },
                Err(err) => debug!("Error while reading a message: {}", err), 
            }
        };

        return None;
    }

    /// Turns a raw IRC message into something easier to process for the client
    fn parse_message(message: Message) -> Option<ChatMessage> {
        match message.command {
            Command::PRIVMSG(_, msg) => {
                if let Some(msgtags) = message.tags { // We should have tags
                    if let Some(prefix) = message.prefix { // We should have a prefix
                        match MessageTagData::from_tags(msgtags) {
                            Ok(tags) => {
                                if let Some(nickname) = Chat::parse_user_name_from_prefix(prefix.as_str()) {
                                    debug!("nickname is {}", nickname);
                                    Some(ChatMessage::Message(
                                        nickname.to_owned(),
                                        msg,
                                        tags,
                                    ))
                                }
                                else {
                                    warn!("PRIVMSG dropped: no nickname");
                                    None
                                }
                            },
                            Err(msg) => {
                                warn!("Error while parsing message tags: {}", msg);
                                None
                            },
                        }
                    }
                    else {
                        warn!("PRIVMSG dropped: no prefix");
                        None
                    }
                }
                else {
                    warn!("PRIVMSG dropped: no tags");
                    None
                }
            },
            Command::CAP(_, sub_command, _, param) => {
                match sub_command {
                    CapSubCommand::ACK => {
                        if let Some(param_str) = param {
                            Some(ChatMessage::Capability(param_str.split_whitespace().map(|s| String::from_str(s).unwrap()).collect()))
                        }
                        else {
                            warn!("The server acknowledged a capability, without saying which one?!?");
                            None
                        }
                    }
                    _ => None,
                }
            },
            Command::MODE(_, mode, nickname_opt) => { 
                if let Some(nickname) = nickname_opt {
                    match mode.as_str() {
                        "+o" => Some(ChatMessage::Operator(nickname, true)),
                        "-o" => Some(ChatMessage::Operator(nickname, false)),
                        _ => None,
                    }
                }
                else {
                    None
                }
            },
            Command::NOTICE(_, content) => {
                if content == "Login authentication failed" {
                    Some(ChatMessage::InvalidAuthToken)
                }
                else {
                    None
                }
            },
            Command::JOIN(_, _, _) => {
                if let Some(nickname) = Chat::parse_user_name_from_message(&message) {
                    Some(ChatMessage::Join(nickname.to_owned()))
                }
                else {
                    warn!("JOIN dropped: no nickname");
                    None
                }
            },
            Command::PART(_, _) => {
                if let Some(nickname) = Chat::parse_user_name_from_message(&message) {
                    Some(ChatMessage::Leave(nickname.to_owned()))
                }
                else {
                    warn!("PART dropped: no nickname");
                    None
                }
            },
            Command::Raw(cmdname, args, suffix) => {
                debug!("Custom command '{}' reveived with args {:?} and suffix {:?}.", cmdname, args, suffix);
                match cmdname.as_str() {
                    "CLEARCHAT" => {
                        // CLEAR 1s
                        // Message received : :tmi.twitch.tv CLEARCHAT #le_shtong :triplepat
                        // Custom command 'CLEARCHAT' reveived with args ["#le_shtong"] and suffix Some("triplepat").

                        // CLEAR global
                        // Message received : :tmi.twitch.tv CLEARCHAT #le_shtong
                        // Custom command 'CLEARCHAT' reveived with args ["#le_shtong"] and suffix None.

                        // Ban
                        // Message received : :tmi.twitch.tv CLEARCHAT #le_shtong :triplepat
                        // Custom command 'CLEARCHAT' reveived with args ["#le_shtong"] and suffix Some("triplepat").

                        // Unban

                        debug!("CLEARCHAT !");
                        None
                    },
                    "ROOMSTATE" => {
                        if let Some(msgtags) = message.tags {
                            Some(ChatMessage::RoomState(RoomStateTags::from_tags_list(msgtags)))
                        }
                        else {
                            None
                        }
                    }
                    &_ => None
                }                
            }
            _ => {
                debug!("Unhandled message type: {:?}", message);
                None
            }
        }
    }

    fn process_message(&mut self, message: ChatMessage) -> bool {
        let start_time = now_utc();
        match message {
            ChatMessage::Message(nickname, msg, tags) => {
                if nickname != self.my_nickname.as_str() { // Ignore messages sent by me
                    self.user_ensure_exists(nickname.as_str());
                    let user_is_protected;
                    let user_is_mod;
                    if let Some(user) = self.all_users.get_mut(nickname.as_str()) {
                        user_is_mod = user.is_mod;

                        // Update user info
                        if let Some(display_name) = tags.display_name {
                            user.display_name = display_name;
                        }

                        if let Some(is_turbo) = tags.is_turbo {
                            if is_turbo {
                                user.is_paying = true;
                            }
                        }

                        if let Some(is_sub) = tags.is_subscriber {
                            if is_sub {
                                user.is_paying = true;
                            }
                        }

                        // TODO: Check if that user bought bits

                        user_is_protected = user_is_mod || // Don't ban mods
                                            user.is_paying || // Don't ban paying users (subs, turbo etc..), they're not bots
                                            user.auto_ban_date.is_some(); // Don't reban unbanned users
                    }
                    else {
                        user_is_mod = false;
                        user_is_protected = false;
                        warn!("Nickname '{}' could not be found!", nickname);
                    }

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
                            if let Some(user) = self.all_users.get_mut(nickname.as_str()) {
                                user.auto_ban_date = Some(now_utc());
                            } 
                            else {
                                warn!("Nickname {} not found for setting its auto-ban date", nickname);
                            }
                        }
                    }
                }
            },
            ChatMessage::Capability(caps) => {
                for cap_name in caps {
                    match cap_name.as_str() {
                        CAP_COMMANDS => self.cap_commands_enabled = true,
                        CAP_MEMBERSHIP => self.cap_membership_enabled = true,
                        CAP_TAGS => self.cap_tags_enabled = true,
                        _ => debug!("Capability {} acknowledged", cap_name),
                    }
                }
            }
            ChatMessage::Operator(nickname, is_op) => {
                self.user_ensure_exists(nickname.as_str());
                if let Some(user) = self.all_users.get_mut(nickname.as_str()) {
                    info!("Setting op mode of '{}' to {}", nickname, is_op);
                    user.is_mod = is_op;
                }
                else {
                    warn!("Nickname '{}' could not be found for setting its mod status", nickname);
                }
            }
            ChatMessage::InvalidAuthToken => {
                error!("The remote server rejected the OAuth token. Make sure it is correct in your configuration file!");
                // We could exit here, but we'll let the connection close by itself
            },
            // ChatMessage::Ban(_, _) => {
            //     // TODO
            // }
            _ => {},
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

    fn parse_user_name_from_message(message: &Message) -> Option<&str> {
        if let Some(ref prefix) = message.prefix {
            Chat::parse_user_name_from_prefix(prefix.as_str())
        }
        else {
            None
        }
    }

    fn parse_user_name_from_prefix(prefix: &str) -> Option<&str> {
        if let Some(pos) = prefix.find('!') {
            Some(&prefix[..pos])
        }
        else {
            info!("Invalid prefix, could not parse. '{}'", prefix);
            None
        }
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_user_name_from_prefix_correct() {
        assert_eq!(Some("MyUser"), Chat::parse_user_name_from_prefix("MyUser!myuser@tmi.twitch.tv"));
    }

    #[test]
    fn parse_user_name_from_prefix_incorrect() {
        assert_eq!(None, Chat::parse_user_name_from_prefix("u wot?"));
    }
}