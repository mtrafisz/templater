use super::cli::{Command, Task};

use sled::Db;
use anyhow::{Result, Context};
use prettytable::{Table, Row, Cell};
use pretty_bytes::converter::convert;
use chrono::{format, DateTime, Local, TimeZone};
use walkdir::WalkDir;
use tar::{Archive, Builder};
use flate2::{Compression, write::GzEncoder, read::GzDecoder};
use globset::{GlobBuilder, GlobMatcher, Glob, GlobSetBuilder};

use std::{
    fs::File,
    path::PathBuf,
    time::SystemTime,
    io::{self}
};

pub mod error;
use error::Error;

mod template;
use template::Template;

pub struct Templater {
    command: Command,
    db: Db,
    storage_path: PathBuf,
}

impl Templater {
    pub fn run_command(command: Command) -> Result<()> {
        let storage_path = dirs::data_local_dir()
            .context("Failed to get config directory")?
            .join("templater");
        let db = sled::Config::new()
            .path(storage_path.join("metadata"))
            .use_compression(true)
            .open()
            .context("Failed to open database")?;

        let mut templater = Templater {
            command,
            db,
            storage_path,
        };

        templater.run().context("Failed to run command")?;
        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        match &self.command.task {
            Task::Create { path, name, description, commands, ignore, force } => {
                self.create_template(path, name, description, commands, ignore, *force).context("Failed to create template")
            }
            Task::Expand { name, path, create_as } => {
                self.expand_template(name, path, create_as)
            }
            Task::List { name } => {
                self.list_templates(name.as_ref())
            }
            Task::Delete { name } => {
                self.delete_template(name)
            }
        }
    }

    fn delete_template(&self, name: &str) -> Result<()> {
        let value = self.db.remove(name)?;
        if value.is_none() {
            return Err(Error::TemplateNotFound(name.to_string()).into());
        }
        if self.command.verbose {
            log::info!("Deleted template metadata: {}", name);
        }
        let archive_path = self.storage_path.join("archives").join(format!("{}.tar.gz", name));
        if archive_path.exists() {
            std::fs::remove_file(archive_path.clone())?;
            if self.command.verbose {
                log::info!("Deleted archive: {}", archive_path.display());
            }
        } else {
            log::warn!("Archive of template {} not found", name);
        }
        Ok(())
    }

    fn list_templates(&self, name: Option<&String>) -> Result<()> {
        let db_iter = self.db.iter();
        let mut empty = true;
        let mut table = Table::new();
        
        table.set_titles(Row::new(vec![
            Cell::new("Name"),
            Cell::new("Description"),
            Cell::new("Compressed Size"),
            Cell::new("Created At"),
            Cell::new("Last Used"),
        ]));

        for item in db_iter {
            let (_key, value) = item?;
            let template: Template = serde_json::from_slice(&value)?;
            
            if let Some(name) = name {
                if name != &template.name {
                    continue;
                }
            }
            empty = false;

            let compressed_size = convert(template.compressed_size as f64);
            let created_at= Local.timestamp_opt(
                template.created.duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap().as_secs() as i64, 0)
                .unwrap().format("%Y-%m-%d %H:%M:%S").to_string();
            let last_used = match template.used {
                Some(time) => Local.timestamp_opt(
                    time.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap().as_secs() as i64, 0)
                    .unwrap().format("%Y-%m-%d %H:%M:%S").to_string(),
                None => "Never".to_string(),
            };

            table.add_row(Row::new(vec![
                Cell::new(&template.name),
                Cell::new(&template.description.unwrap_or_default()),
                Cell::new(&compressed_size),
                Cell::new(&created_at),
                Cell::new(&last_used),
            ]));
        }

        if empty {
            log::info!("No templates found");
        } else {
            table.printstd();
        }
    
        Ok(())
    }

    fn expand_template(&self, name: &str, path: &Option<PathBuf>, create_as: &Option<String>) -> Result<()> {
        let mut template: Template = match self.db.get(name)? {
            Some(data) => serde_json::from_slice(&data)?,
            None => return Err(Error::TemplateNotFound(name.to_string()).into()),
        };

        template.used = Some(SystemTime::now());
        self.db.insert(name, serde_json::to_vec(&template)?)?;
        let template = template;    // unmut

        let path = match path {
            Some(path) => path.clone(),
            None => PathBuf::from("."),
        };

        let create_as = match create_as {
            Some(create_as) => create_as.clone(),
            None => name.to_string(),
        };

        if self.command.verbose {
            log::info!("Expanding template {} to {}", name, path.display());
        }

        let archive_path = self.storage_path.join("archives").join(format!("{}.tar.gz", name));
        let archive = File::open(&archive_path)?;
        let dec = GzDecoder::new(archive);
        let mut archive = Archive::new(dec);

        let new_path = path.join(&create_as);
        if new_path.exists() {
            return Err(Error::InvalidTemplateDir(new_path).into());
        }

        std::fs::create_dir_all(&new_path)?;
        if self.command.verbose {
            log::info!("Creating directory: {}", new_path.display());
        }
        archive.unpack(&new_path)?;
        if self.command.verbose {
            log::info!("Unpacked archive: {}", archive_path.display());
        }

        let cwd = std::env::current_dir()?;

        std::env::set_current_dir(&new_path)?;
        for command in template.commands {
            let mut parts = command.split_whitespace();
            let command = parts.next().unwrap();
            let args = parts.collect::<Vec<&str>>();

            if self.command.verbose {
                log::info!("Running command: {} {}", command, args.join(" "));
            }

            // sh -c "command args"
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("{} {}", command, args.join(" ")))
                .status()?;

            if !status.success() {
                return Err(Error::CreateTemplate(command.to_string()).into());
            }
        }
        std::env::set_current_dir(&cwd)?;

        Ok(())
    }

    fn create_template(&self, path: &PathBuf, name: &Option<String>, description: &Option<String>, commands: &Vec<String>, ignore: &Vec<String>, force: bool) -> Result<()> {
        if !path.exists() || !path.is_dir() {
            return Err(Error::InvalidTemplateDir(path.clone()).into());
        }

        let name = match name {
            Some(name) => name.clone(),
            None => path.file_name().context("Failed to get file name")?.to_string_lossy().to_string(),
        };

        if self.db.contains_key(&name)? && !force {
            return Err(Error::TemplateExists(name).into());
        }

        if self.command.verbose {
            log::info!("Creating archive file for template: {}", name);
        }

        let archive_path = self.storage_path.join("archives").join(format!("{}.tar.gz", name));
        std::fs::create_dir_all(&archive_path.parent().unwrap())?;
        if self.command.verbose {
            log::info!("Creted archive directory: {}", archive_path.parent().unwrap().display());
        }

        let tarball = File::create(&archive_path).context("Failed to create archive")?;
        if self.command.verbose {
            log::info!("Created archive file: {}", archive_path.display());
        }

        let enc = GzEncoder::new(tarball, Compression::default());
        let mut tar = Builder::new(enc);

        let ignore_list = ignore.iter()
            .map(|pattern| {
                let mut builder = GlobBuilder::new(pattern);
                builder.case_insensitive(true);
                builder.build()
                    .context(format!("Failed to build glob pattern: {}", pattern))
                    .map(|glob| glob.compile_matcher())
            })
            .collect::<Result<Vec<GlobMatcher>>>()?;
        
        if self.command.verbose {
            log::info!("Filtering files with ignore patterns: {:?}", ignore);
        }

        let file_path_list = WalkDir::new(path)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path().to_path_buf())
            .filter(|path| {
                !ignore_list.iter().any(|matcher| matcher.is_match(path.to_str().unwrap()))
            });

        for file_path in file_path_list {
            let relative_path = PathBuf::from("./").join(file_path.strip_prefix(path).unwrap());

            if self.command.verbose {
                log::info!("Adding path to archive: {}", relative_path.display());
            }

            if file_path.is_file() {
                let mut file = File::open(&file_path).context(format!("Failed to open file: {}", file_path.display()))?;
                tar.append_file(relative_path, &mut file)?;
            } else {
                tar.append_dir(relative_path, &file_path)?;
            }
        }

        tar.finish()?;
        drop(tar);

        if self.command.verbose {
            log::info!("Finished creating archive: {}", archive_path.display());
        }

        let metadata = std::fs::metadata(&archive_path).context(format!("Failed to get metadata: {}", archive_path.display()))?;
        let compressed_size = metadata.len();

        let template = Template {
            name: name.clone(),
            description: description.clone(),
            commands: commands.clone(),
            compressed_size,
            ignore: ignore.clone(),
            created: SystemTime::now(),
            used: None,
        };

        if self.command.verbose {
            log::info!("Creating metadata for template: {}", name);
        }

        let value = serde_json::to_string(&template).context("Failed to serialize template")?;
        self.db.insert(&name, value.as_bytes())?;

        if self.command.verbose {
            log::info!("Finished creating template: {}", name);
        }

        Ok(())
    }
}
