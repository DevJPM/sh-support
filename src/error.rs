use crate::PlayerID;
use std::{fmt, io, str};

#[derive(Debug)]
pub(crate) enum Error {
    BadExecutable(String),
    BadPlayerID(PlayerID),
    ParsePolicyError(String),
    ParseRoleError(String),
    ParseNameError(String),
    FileSystemError(io::Error),
    TooLongPatternError { have : usize, requested : usize },
    TooShortPatternError { have : usize, requested : usize },
    ReplError(repl_rs::Error),
    LogicalInconsistency,
    BadFactIndex(usize) //ImpossibleConflict
}

impl From<repl_rs::Error> for Error {
    fn from(error : repl_rs::Error) -> Self { Error::ReplError(error) }
}

impl From<str::Utf8Error> for Error {
    fn from(error : str::Utf8Error) -> Self { Error::ParsePolicyError(error.to_string()) }
}

impl From<io::Error> for Error {
    fn from(error : io::Error) -> Self { Error::FileSystemError(error) }
}

impl fmt::Display for Error {
    fn fmt(&self, f : &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Error::TooLongPatternError { have, requested } => write!(
                f,
                "Requested a pattern of length {requested} but only had {have} cards available.",
            ),
            Error::TooShortPatternError { have, requested } => write!(
                f,
                "Presented a pattern of length {requested} but the required pattern length is \
                 {have}.",
            ),
            Error::ParsePolicyError(found) => write!(
                f,
                "Failed to parse single-letter policy name, found {found} instead."
            ),
            Error::ParseRoleError(found) => {
                write!(f, "Failed to parse role name name, found {found} instead.")
            },
            Error::ReplError(error) => write!(f, "{error}"),
            Error::BadPlayerID(id) => write!(f, "Failed to recognize player {id}."),
            Error::FileSystemError(fserror) => write!(f, "Filesystem error: {fserror}"),
            Error::LogicalInconsistency => write!(
                f,
                "Detected a logical inconsistency, check your fact database to debug it."
            ),
            Error::ParseNameError(name) => {
                write!(f, "Failed to associate \"{name}\" with a player's name.")
            },
            Error::BadFactIndex(index) => write!(f, "Fact #{index} does not exist."),
            Error::BadExecutable(executable) => write!(
                f,
                "Found an unexpected dot invocation strategy in {executable}."
            )
        }
    }
}

impl std::error::Error for Error {}
