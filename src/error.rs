use image::ImageError;

use crate::PlayerID;
use std::{fmt, io, str};

#[derive(Debug)]
pub(crate) enum Error {
    BadExecutable(String),
    UnexpectedStdout(Vec<u8>),
    UnexpectedStderr(Vec<u8>),
    ImageError(ImageError),
    EncodingError,
    ClipBoardError(arboard::Error),
    BadPlayerID(PlayerID),
    DeadPlayerID(PlayerID),
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

impl From<ImageError> for Error {
    fn from(e : ImageError) -> Self { Error::ImageError(e) }
}

impl From<arboard::Error> for Error {
    fn from(e : arboard::Error) -> Self { Error::ClipBoardError(e) }
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
            ),
            Error::UnexpectedStdout(out) => write!(
                f,
                "Found an unexpected stdout output: {}",
                String::from_utf8_lossy(out)
            ),
            Error::UnexpectedStderr(err) => write!(
                f,
                "Found an unexpected stderr output: {}",
                String::from_utf8_lossy(err)
            ),
            Error::ImageError(e) => write!(f, "{e}"),
            Error::ClipBoardError(e) => write!(f, "{e}"),
            Error::EncodingError => write!(
                f,
                "Failed to encode the output png image into the format for the clipboard."
            ),
            Error::DeadPlayerID(killed) => write!(f,"Player {killed} cannot be selected here because they are dead."),
        }
    }
}

impl std::error::Error for Error {}
