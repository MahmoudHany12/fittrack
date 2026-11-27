use anyhow::{bail, Context};
use chrono::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_access_secret: String,
    pub bind_addr: String,
    pub access_ttl: Duration,
    pub refresh_ttl: Duration,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let jwt_access_secret =
            std::env::var("JWT_ACCESS_SECRET").context("JWT_ACCESS_SECRET is required")?;
        if jwt_access_secret.len() < 32 || jwt_access_secret == "CHANGE_ME" {
            bail!("JWT_ACCESS_SECRET must be at least 32 characters");
        }
        Ok(Self {
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL is required")?,
            jwt_access_secret,
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            access_ttl: Duration::minutes(15),
            refresh_ttl: Duration::days(30),
        })
    }

    pub fn for_test() -> Self {
        Self {
            database_url: String::new(),
            jwt_access_secret: "test-secret-test-secret-test-secret-123".into(),
            bind_addr: "127.0.0.1:0".into(),
            access_ttl: Duration::minutes(15),
            refresh_ttl: Duration::days(30),
        }
    }
}
