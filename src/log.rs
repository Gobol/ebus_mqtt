use std::fmt::{self, Display, Formatter};

use crate::LOG_LEVEL;


#[derive(Debug, Copy, Clone)]
pub enum LogLevel {
    Debug = 0,
    Info,
    Warning,
    Error
}

impl Display for LogLevel {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "[D]"),
            LogLevel::Info => write!(f, "[I]"),
            LogLevel::Warning => write!(f, "[W]"),
            LogLevel::Error => write!(f, "[E]")
        }
    }

}

// logging
pub fn log<S: Into<String> + std::fmt::Display>(level: LogLevel, message: S) {
    if level as u8 >= LOG_LEVEL as u8 {
        print!("{} {}", level, message);
    }
}

pub fn logln<S: Into<String> + std::fmt::Display>(level: LogLevel, message: S) {
    log(level, message);
    println!();
}

pub fn logD<S: Into<String> + std::fmt::Display>(message: S) {
    log(LogLevel::Debug, message);
}
pub fn logI<S: Into<String> + std::fmt::Display>(message: S) {
    log(LogLevel::Info, message);
}
pub fn logW<S: Into<String> + std::fmt::Display>(message: S) {
    log(LogLevel::Warning, message);
}
pub fn logE<S: Into<String> + std::fmt::Display>(message: S) {
    log(LogLevel::Error, message);
}

pub fn logDln<S: Into<String> + std::fmt::Display>(message: S) {
    logln(LogLevel::Debug, message);
}   
pub fn logIln<S: Into<String> + std::fmt::Display>(message: S) {
    logln(LogLevel::Info, message);
}   
pub fn logWln<S: Into<String> + std::fmt::Display>(message: S) {
    logln(LogLevel::Warning, message);
}   
pub fn logEln<S: Into<String> + std::fmt::Display>(message: S) {
    logln(LogLevel::Error, message);
}   
