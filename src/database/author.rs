use std::fmt::Display;

use chrono::{DateTime, Utc};

#[derive(Clone, Debug)]
pub struct Author {
    name: String,
    email: String,
    time: DateTime<Utc>,
}

impl Author {
    pub fn new(name: String, email: String, time: DateTime<Utc>) -> Self {
        Self { name, email, time }
    }
}

impl Display for Author {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} <{}> {}",
            self.name,
            self.email,
            self.time.format("%s %z")
        )
    }
}
