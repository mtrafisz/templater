use super::Error;

use serde::{Deserialize, Serialize};

use anyhow::{Result, Context};

use std::{
    fs::File,
    path::PathBuf,
    process::Command,
    time::SystemTime,
    io::{self}
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    pub description: Option<String>,
    pub commands: Vec<String>,
    pub compressed_size: u64,
    pub ignore: Vec<String>,
    pub created: SystemTime,
    pub used: Option<SystemTime>,
}
