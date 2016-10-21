pub struct Checker {
    ban_list: Vec<&'static str>,
}

impl Checker {
    pub fn new() -> Checker {
        // Fill up the list with some dumb strings
        Checker{
            ban_list: vec![
                "ban me!",
                "hello",
            ],
        }
    }

    pub fn check(&self, input: &str) -> bool {
        self.ban_list.iter().any(|&expr| expr == input)
    }
}