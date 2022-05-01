use crate::Error;
use std::{fmt, str};

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) enum SecretRole {
    Liberal,
    RegularFascist,
    Hitler
}

impl fmt::Display for SecretRole {
    fn fmt(&self, f : &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretRole::Liberal => write!(f, "Liberal"),
            SecretRole::RegularFascist => write!(f, "Fascist"),
            SecretRole::Hitler => write!(f, "Hitler")
        }
    }
}

impl str::FromStr for SecretRole {
    type Err = Error;

    fn from_str(s : &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "h" | "hitler" => Ok(SecretRole::Hitler),
            "f" | "fascist" => Ok(SecretRole::RegularFascist),
            "l" | "b" | "lib" | "blue" | "liberal" => Ok(SecretRole::Liberal),
            _ => Err(Error::ParseRoleError(s.to_owned()))
        }
    }
}

impl SecretRole {
    pub fn is_fascist(&self) -> bool { !matches!(self, SecretRole::Liberal) }
}
