use crate::constants::*;
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: String,
    pub data_path: String,
    pub session_secret: String,
}

#[derive(Debug)]
pub enum ConfigError {
    MissingSessionSecret,
    InvalidSessionSecret(String),
    InvalidPort(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingSessionSecret => {
                write!(f, "SESSION_SECRET environment variable is required")
            }
            ConfigError::InvalidSessionSecret(msg) => {
                write!(f, "Invalid session secret: {}", msg)
            }
            ConfigError::InvalidPort(port) => {
                write!(f, "Invalid port number: {}", port)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let host = env::var("SERVER_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
        let port = env::var("SERVER_PORT").unwrap_or_else(|_| DEFAULT_PORT.to_string());
        let data_path = env::var("DATABASE_PATH").unwrap_or_else(|_| DEFAULT_DATA_PATH.to_string());

        // Validate port is a valid number
        if port.parse::<u16>().is_err() {
            return Err(ConfigError::InvalidPort(port));
        }

        // Get and validate session secret
        let session_secret =
            env::var("SESSION_SECRET").map_err(|_| ConfigError::MissingSessionSecret)?;

        if session_secret.len() < MIN_SESSION_SECRET_LENGTH {
            return Err(ConfigError::InvalidSessionSecret(format!(
                "must be at least {} characters long",
                MIN_SESSION_SECRET_LENGTH
            )));
        }

        Ok(Config {
            host,
            port,
            data_path,
            session_secret,
        })
    }

    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
