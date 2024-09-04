use super::cli::{Command, Task};

use anyhow::{Context, Result};
use chrono::{Local, TimeZone};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use globset::{GlobBuilder, GlobMatcher};
use pretty_bytes::converter::convert;
use prettytable::{Cell, Row, Table};
use serde::{Deserialize, Serialize};
use sled::Db;
use tar::{Archive, Builder};
use walkdir::WalkDir;

use std::{
    collections::HashMap, fs::File, io::{Read, Seek, Write}, path::PathBuf, time::SystemTime
};

pub mod error;
use error::Error;

#[derive(Debug, Serialize, Deserialize)]
struct Template {
    pub name: String,
    pub description: Option<String>,
    pub commands: Vec<String>,
    pub compressed_size: u64,
    pub created: SystemTime,
    pub used: Option<SystemTime>,
}

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
            Task::Create {
                path,
                name,
                description,
                commands,
                ignore,
                definition_file,
                force,
            } => self
                .create_template(path, name, description, commands, ignore, definition_file, *force)
                .context("Failed to create template"),
            Task::Expand {
                name,
                path,
                envs,
                create_as,
                no_exec,
            } => self
                .expand_template(name, path, envs, create_as, no_exec)
                .context("Failed to expand template"),
            Task::List { name, commands, file_tree } => {
                if name.is_none() && *commands {
                    return Err(Error::InvalidArgument(
                        "You can only list commands for a specific template, please provide --name"
                            .to_string(),
                    )
                    .into());
                }

                if name.is_none() && *file_tree {
                    return Err(Error::InvalidArgument(
                        "You can only display file tree for a specific template, please provide --name"
                            .to_string(),
                    )
                    .into());
                }

                self.list_templates(name.as_ref())?;
                if *commands {
                    self.list_commands(name.as_ref().unwrap())?;
                }
                if *file_tree {
                    self.show_file_tree(name.as_ref().unwrap())?;
                }
                Ok(())
            }
            Task::Delete { name } => self.delete_template(name),
            Task::Edit { name } => self.edit_template(name),
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
        let archive_path = self
            .storage_path
            .join("archives")
            .join(format!("{}.tar.gz", name));
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
                if !template.name.contains(name) {
                    continue;
                }
            }
            empty = false;

            let compressed_size = convert(template.compressed_size as f64);
            let created_at = Local
                .timestamp_opt(
                    template
                        .created
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                    0,
                )
                .unwrap()
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();
            let last_used = match template.used {
                Some(time) => Local
                    .timestamp_opt(
                        time.duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as i64,
                        0,
                    )
                    .unwrap()
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
                None => "Never".to_string(),
            };

            table.add_row(Row::new(vec![
                Cell::new(&template.name),
                Cell::new(&template.description.unwrap_or("No description".to_string())),
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

    fn list_commands(&self, name: &str) -> Result<()> {
        let template: Template = match self.db.get(name)? {
            Some(data) => serde_json::from_slice(&data)?,
            None => return Err(Error::TemplateNotFound(name.to_string()).into()),
        };

        let commands = template
            .commands
            .iter()
            .fold("Commands:".to_string(), |acc, command| {
                format!("{}\n{}", acc, command)
            });
        log::info!("{}", commands);

        Ok(())
    }

    // TODO: This should not be in this project, isn't there any crate that can do that for me?
    fn print_tar_tree<R: std::io::Read>(archive: &mut Archive<R>) -> Result<()> {
        let mut dir_stack = vec![("".to_string(), 0)];
        let mut last_depth = 0;
        let mut is_first_entry = true;
    
        let entries: Vec<_> = archive.entries()?.collect();
        let total_entries = entries.len();
    
        for (i, entry_result) in entries.into_iter().enumerate() {
            let entry = entry_result?;
            let path = entry.path()?;
            let depth = path.components().count();
    
            while let Some((_, stack_depth)) = dir_stack.last() {
                if *stack_depth >= depth {
                    dir_stack.pop();
                } else {
                    break;
                }
            }
    
            let file_name = path.file_name().unwrap_or_else(|| path.as_os_str()).to_string_lossy();
    
            let def_prefix = &(String::new(), 0);
            let (prefix, _) = dir_stack.last().unwrap_or(&def_prefix);
            let connector = if i == total_entries - 1 || (depth != last_depth && !is_first_entry) {
                "└── "
            } else {
                "├── "
            };
    
            if is_first_entry {
                println!("{}", file_name);
                is_first_entry = false;
            } else {
                println!("{}{}{}", prefix, connector, file_name);
            }
    
            if entry.header().entry_type().is_dir() {
                let next_prefix = if i == total_entries - 1 || (depth != last_depth) {
                    format!("{}    ", prefix)
                } else {
                    format!("{}│   ", prefix)
                };
                dir_stack.push((next_prefix, depth));
            }
    
            last_depth = depth;
        }
        Ok(())
    }

    fn show_file_tree(&self, name: &str) -> Result<()> {
        let archive_path = self
            .storage_path
            .join("archives")
            .join(format!("{}.tar.gz", name));
        let archive_file = std::fs::File::open(archive_path)?;

        let decoder = GzDecoder::new(archive_file);
        let mut archive = Archive::new(decoder);

        log::info!("File Tree");
        Self::print_tar_tree(&mut archive)
    }

    fn expand_template(
        &self,
        name: &str,
        path: &Option<PathBuf>,
        envs: &Vec<String>,
        create_as: &Option<String>,
        no_exec: &bool,
    ) -> Result<()> {
        let mut template: Template = match self.db.get(name)? {
            Some(data) => serde_json::from_slice(&data)?,
            None => return Err(Error::TemplateNotFound(name.to_string()).into()),
        };

        template.used = Some(SystemTime::now());
        self.db.insert(name, serde_json::to_vec(&template)?)?;
        let template = template; // unmut

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

        let archive_path = self
            .storage_path
            .join("archives")
            .join(format!("{}.tar.gz", name));
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

        if *no_exec {
            return Ok(());
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

            let envs: HashMap<String, String> = envs
                .iter()
                .map(|env| {
                    let mut parts = env.split("=");
                    let key = parts.next().unwrap();
                    let value = parts.collect::<Vec<&str>>().join("=");
                    (key.to_string(), value)
                })
                .collect();

            let status = if cfg!(target_os = "windows") {
                std::process::Command::new("cmd")
                    .arg("/C")
                    .arg(command)
                    .args(args)
                    .envs(envs.iter())
                    .status()?
            } else {
                std::process::Command::new("sh")
                    .arg("-c")
                    .arg(format!("{} {}", command, args.join(" ")))
                    .envs(envs.iter())
                    .status()?
            };

            if !status.success() {
                return Err(Error::CreateTemplate(command.to_string()).into());
            }
        }
        std::env::set_current_dir(&cwd)?;

        Ok(())
    }

    fn create_template(
        &self,
        path: &PathBuf,
        name: &Option<String>,
        description: &Option<String>,
        commands: &Vec<String>,
        ignore: &Vec<String>,
        definition: &Option<PathBuf>,
        force: bool,
    ) -> Result<()> {
        if !path.exists() || !path.is_dir() {
            return Err(Error::InvalidTemplateDir(path.clone()).into());
        }

        /* The most discusting code I've ever written is here */

        // definition parsing
        #[derive(Serialize, Deserialize)]
        struct TemplateDefinition {
            name: Option<String>,
            description: Option<String>,
            commands: Vec<String>,
            ignore: Vec<String>
        }

        impl TemplateDefinition {
            fn load_template_definition(path: &PathBuf) -> Option<Self> {
                let def_file = std::fs::File::open(path);
                match def_file {
                    Err(e) => {
                        log::error!("Couldn't open definition file {}: {e}", path.display());
                        return None;
                    }
                    Ok(mut f) => {
                        let mut contents = String::new();
                        if let Err(e) = f.read_to_string(&mut contents) {
                            log::error!("Couldn't read from definition file: {e}");
                            return None;
                        }

                        match serde_json::from_str(&contents) {
                            Ok(template_def) => Some(template_def),
                            Err(e) => {
                                eprintln!("Provided file is not valid definition file: {e}");
                                None
                            }
                        }
                    }
                }
            }
        }

        let definition_contents = {
            match definition {
                None => {None},
                Some(d) => {
                    TemplateDefinition::load_template_definition(d)
                }
            }
        };

        let config = match definition_contents {
            Some(d) => {
                let name = match name {
                    Some(n) => n.clone(),
                    None => {
                        match d.name {
                            Some(n) => n.clone(),
                            None => {
                                path
                                .file_name()
                                .context("Failed to get file name")?
                                .to_string_lossy()
                                .to_string()
                            }
                        }
                    }
                };

                let description = match description {
                    Some(desc) => Some(desc.clone()),
                    None => {
                        match d.description {
                            Some(desc) => Some(desc.clone()),
                            None => None
                        }
                    }
                };

                let commands = {
                    if commands.len() == 0 && d.commands.len() != 0 {
                        d.commands
                    } else {
                        commands.to_vec()
                    }
                };

                let ignore = {
                    if ignore.len() == 0 && d.ignore.len() != 0 {
                        d.ignore
                    } else {
                        ignore.to_vec()
                    }
                };

                TemplateDefinition {
                    name: Some(name),
                    description,
                    commands,
                    ignore,
                }
            },
            None => {
                let name = match name {
                    Some(n) => n.clone(),
                    None => {
                        path
                        .file_name()
                        .context("Failed to get file name")?
                        .to_string_lossy()
                        .to_string()
                    }
                };

                TemplateDefinition {
                    name: Some(name),
                    description: description.clone(),
                    commands: commands.clone(),
                    ignore: ignore.clone(),
                }
            }
        };

        /* End of very very bad code */

        let name = config.name.unwrap();

        if self.db.contains_key(&name)? && !force {
            return Err(Error::TemplateExists(name).into());
        }

        if self.command.verbose {
            log::info!("Creating archive file for template: {}", name);
        }

        let archive_path = self
            .storage_path
            .join("archives")
            .join(format!("{}.tar.gz", name));
        std::fs::create_dir_all(&archive_path.parent().unwrap())?;
        if self.command.verbose {
            log::info!(
                "Creted archive directory: {}",
                archive_path.parent().unwrap().display()
            );
        }

        let tarball = File::create(&archive_path).context("Failed to create archive")?;
        if self.command.verbose {
            log::info!("Created archive file: {}", archive_path.display());
        }

        let enc = GzEncoder::new(tarball, Compression::default());
        let mut tar = Builder::new(enc);

        let ignore_list = config.ignore
            .iter()
            .map(|pattern| {
                let mut builder = GlobBuilder::new(pattern);
                builder.case_insensitive(true);
                builder
                    .build()
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
                !ignore_list
                    .iter()
                    .any(|matcher| matcher.is_match(path.to_str().unwrap()))
            });

        for file_path in file_path_list {
            let relative_path = PathBuf::from("./").join(file_path.strip_prefix(path).unwrap());

            if self.command.verbose {
                log::info!("Adding path to archive: {}", relative_path.display());
            }

            if file_path.is_file() {
                let mut file = File::open(&file_path)
                    .context(format!("Failed to open file: {}", file_path.display()))?;
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

        let metadata = std::fs::metadata(&archive_path).context(format!(
            "Failed to get metadata: {}",
            archive_path.display()
        ))?;
        let compressed_size = metadata.len();

        let template = Template {
            name: name.clone(),
            description: config.description.clone(),
            commands: config.commands.clone(),
            compressed_size,
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

    fn edit_template(&self, name: &str) -> Result<()> {
        let template: Template = match self.db.get(name)? {
            Some(data) => serde_json::from_slice(&data)?,
            None => return Err(Error::TemplateNotFound(name.to_string()).into()),
        };

        let editor = std::env::var("EDITOR").unwrap_or("vi".to_string());
        let mut file = tempfile::NamedTempFile::new()?;

        #[derive(Serialize, Deserialize)]
        struct TemplateEditFile {
            name: String,
            description: Option<String>,
            commands: Vec<String>,
        }

        let template_edit_file = TemplateEditFile {
            name: template.name.clone(),
            description: template.description.clone(),
            commands: template.commands.clone(),
        };

        file.write_all(serde_json::to_string_pretty(&template_edit_file)?.as_bytes())?;

        let status = std::process::Command::new(editor)
            .arg(file.path())
            .status()?;
        if !status.success() {
            return Err(Error::EditTemplate("Failed to open editor".to_string()).into());
        }

        file.seek(std::io::SeekFrom::Start(0))?;
        let mut contents = String::new();

        file.read_to_string(&mut contents)?;

        let template_edit: TemplateEditFile = serde_json::from_str(&contents)?;
        let template = Template {
            name: template_edit.name,
            description: template_edit.description,
            commands: template_edit.commands,
            compressed_size: template.compressed_size,
            created: template.created,
            used: template.used,
        };

        self.db.insert(name, serde_json::to_vec(&template)?)?;

        let archive_path = self
            .storage_path
            .join("archives")
            .join(format!("{}.tar.gz", name));
        let new_archive_path = self
            .storage_path
            .join("archives")
            .join(format!("{}.tar.gz", template.name));
        std::fs::rename(archive_path, new_archive_path)?;

        self.list_templates(Some(&template.name))?;
        self.list_commands(&template.name)?;

        Ok(())
    }
}
