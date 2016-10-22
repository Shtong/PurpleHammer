use std::io::{Error, Read};
use std::fs::File;
use std::path::Path;

use irc::client::data::Config as IrcConfig;
use yaml_rust::YamlLoader;
use yaml_rust::yaml::Yaml;
use yaml_rust::scanner::ScanError;

pub struct HammerConfig {
    username: Option<String>,
    oauth: Option<String>,
    channel: Option<String>,
    owners: Option<Vec<String>>,
}

impl HammerConfig {
    pub fn new() -> HammerConfig {
        HammerConfig {
            username: None,
            oauth: None,
            channel: None,
            owners: None,
        }
    }

    pub fn fill_from_file<P: AsRef<Path>>(&mut self, source: P) -> Result<(), Error> {
        let mut file = try!(File::open(source));
        let mut file_text = String::new();
        try!(file.read_to_string(&mut file_text));
        // TODO : Better error handling
        self.fill_from_string(&file_text).unwrap();
        Ok(())
    }

    fn fill_from_string(&mut self, source: &str) -> Result<(), ScanError> {
        let data = try!(YamlLoader::load_from_str(source));
        self.fill_from_yaml(&data);
        Ok(())
    }

    fn fill_from_yaml(&mut self, source: &Vec<Yaml>) {
        for entry in source {
            match entry {
                &Yaml::Hash(ref h) => {
                    for(k, v) in h {
                        match k {
                            &Yaml::String(ref keyval) => {
                                match keyval.as_ref() {
                                    "username" => self.username = HammerConfig::read_string(v, "username"),
                                    "oauth" => self.oauth = HammerConfig::read_string(v, "oauth"),
                                    "channel" => self.channel = HammerConfig::read_string(v, "channel"),
                                    "owners" => self.owners = HammerConfig::read_owner_list(v),
                                    &_ => debug!("CONFIG: Unknown key '{}'", keyval),
                                }
                            },
                            _ => debug!("CONFIG : Non-string key found; skipped ({:?})", k)
                        }
                    }
                }
                _ => debug!("CONFIG : A non-hash entry was skipped at the root level")
            }
        }
    }

    fn read_owner_list(token : &Yaml) -> Option<Vec<String>> {
        match token {
            &Yaml::String(ref value) => Some(vec![value.clone()]),
            &Yaml::Array(ref value) => {
                let mut list = Vec::new();
                for owner in value {
                    match owner {
                        &Yaml::String(ref owner_name) => list.push(owner_name.clone()),
                        &_ => warn!("CONFIG: An entry in the owner list was not a string, and was skipped ({:?})", owner),
                    }
                }
                Some(list)
            },
            _ => {
                warn!("CONFIG: The owners entry contains an invalid type. Only a string or a list of strings are supported");
                None
            }
        }
    }

    fn read_string(token: &Yaml, val_key: &str) -> Option<String> {
        match token {
            &Yaml::String(ref value) => Some(value.clone()),
            _ => {
                debug!("CONFIG : Value in key {} should be a string but is not! ({:?})", val_key, token);
                None
            }
        }
    }

    pub fn to_irc_config(&self) -> IrcConfig {
        // Copy the values over
        let mut result = IrcConfig {
            server: Some(format!("irc.chat.twitch.tv")),
            port: Some(6667),
            nickname: self.username.clone(),
            password: self.oauth.clone(),
            .. Default::default()
        };

        if let Some(ref channel_name) = self.channel {
            result.channels = Some(vec![format!("#{}", channel_name.to_lowercase())]);
        }

        if let Some(ref owners_names) = self.owners {
            result.owners = Some(owners_names.iter().cloned().collect());
        }

        result
    }
}