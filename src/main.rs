extern crate irc;

mod checker;

use std::default::Default;
use irc::client::prelude::*;

fn main() {
    let irc_config = Config {
        server: Some(format!("irc.chat.twitch.tv")),
        port: Some(6667),
        channels: Some(vec![format!("#le_shtong")]),
        nickname: Some(format!("Purple_Hammer")),
        password: Some(format!("oauth:78sfum5mlfghts0u0y8jqc6sg52ppx")),
        owners: Some(vec![format!("Le_Shtong")]),
        .. Default::default()
    };

    println!("Connecting to IRC...");
    let server = IrcServer::from_config(irc_config).unwrap();
    server.identify().unwrap();
    println!("Connected!");

    let my_checker = checker::Checker::new();

    for message in server.iter() {
        let message = message.unwrap();
        println!("Message received : {}", message);
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
