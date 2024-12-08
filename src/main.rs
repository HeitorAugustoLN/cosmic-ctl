#[cfg(test)]
mod tests;

use bracoxide::explode;
use clap::{Parser, Subcommand};
use glob::Pattern;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env, fs,
    io::{Error, ErrorKind, Write},
    path::{Path, PathBuf},
};
use unescaper::unescape;
use walkdir::WalkDir;

/// CLI for COSMIC Desktop configuration management
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Write a configuration entry.
    #[command(disable_version_flag = true)]
    Write {
        /// The configuration version of the component.
        #[arg(short, long, default_value_t = 1)]
        version: u64,
        /// The component to configure (e.g., 'com.system76.CosmicComp').
        #[arg(short, long)]
        component: String,
        /// The specific configuration entry to modify (e.g., 'autotile').
        #[arg(short, long)]
        entry: String,
        /// The value to assign to the configuration entry. (e.g., 'true').
        value: String,
    },
    /// Read a configuration entry.
    #[command(disable_version_flag = true)]
    Read {
        /// The configuration version of the component.
        #[arg(short, long, default_value_t = 1)]
        version: u64,
        /// The component to configure (e.g., 'com.system76.CosmicComp').
        #[arg(short, long)]
        component: String,
        /// The specific configuration entry to modify (e.g., 'autotile').
        #[arg(short, long)]
        entry: String,
    },
    /// Delete a configuration entry.
    #[command(disable_version_flag = true)]
    Delete {
        /// The configuration version of the component.
        #[arg(short, long, default_value_t = 1)]
        version: u64,
        /// The component to configure (e.g., 'com.system76.CosmicComp').
        #[arg(short, long)]
        component: String,
        /// The specific configuration entry to modify (e.g., 'autotile').
        #[arg(short, long)]
        entry: String,
    },
    /// Write configurations from a JSON file.
    Apply {
        /// Path to the JSON file containing configuration entries.
        file: PathBuf,
        /// Print verbose output about skipped entries.
        #[arg(short, long)]
        verbose: bool,
    },
    /// Backup all configuration entries to a JSON file.
    Backup {
        /// Path to the output JSON file.
        file: PathBuf,
        /// Show which entries are being backed up.
        #[arg(short, long)]
        verbose: bool,
    },
    /// Delete all configuration entries.
    Reset {
        /// Skip confirmation prompt.
        #[arg(short, long)]
        force: bool,
        /// Show which entries are being deleted.
        #[arg(short, long)]
        verbose: bool,
        /// Patterns to exclude from reset (comma-separated).
        #[arg(long)]
        exclude: Option<String>,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Operation {
    Write,
    Read,
    Delete,
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
enum EntryContent {
    WriteEntries(HashMap<String, String>),
    ReadDeleteEntries(Vec<String>),
}

#[derive(Deserialize, Serialize)]
struct Entry {
    component: String,
    version: u64,
    operation: Operation,
    entries: EntryContent,
}

#[derive(Deserialize, Serialize)]
struct ConfigFile {
    #[serde(rename = "$schema")]
    schema: String,
    operations: Vec<Entry>,
}

fn main() {
    let cli: Cli = Cli::parse();

    match &cli.command {
        Commands::Write {
            component,
            version,
            entry,
            value,
        } => match write_configuration(component, version, entry, value) {
            Ok(true) => println!("Configuration entry written successfully."),
            Ok(false) => println!("Doing nothing, entry already has this value."),
            Err(e) => eprintln!("Error writing configuration: {}", e),
        },
        Commands::Read {
            version,
            component,
            entry,
        } => match read_configuration(component, version, entry) {
            Ok(contents) => println!("{}", contents),
            Err(e) => eprintln!("Error reading configuration: {}", e),
        },
        Commands::Delete {
            version,
            component,
            entry,
        } => match delete_configuration(component, version, entry) {
            Ok(()) => println!("Configuration entry deleted successfully."),
            Err(e) => eprintln!("Error: {}", e),
        },
        Commands::Apply { file, verbose } => {
            if file.extension().and_then(|s| s.to_str()) != Some("json") {
                eprintln!("Error: The file is not in JSON format.");
                return;
            }

            let file_content = fs::read_to_string(file).expect("Unable to read file");
            let config_file: ConfigFile =
                serde_json::from_str(&file_content).expect("Invalid JSON format");

            let mut write_changes = 0;
            let mut read_count = 0;
            let mut delete_count = 0;
            let mut skipped = 0;

            for entry in config_file.operations {
                match (entry.operation, entry.entries) {
                    (Operation::Write, EntryContent::WriteEntries(entries)) => {
                        for (key, value) in entries {
                            match write_configuration(
                                &entry.component,
                                &entry.version,
                                &key,
                                &value,
                            ) {
                                Ok(false) => {
                                    if *verbose {
                                        println!(
                                            "Skipping {}/v{}/{} - value unchanged",
                                            entry.component, entry.version, key
                                        );
                                    }
                                    skipped += 1;
                                }
                                Ok(true) => write_changes += 1,
                                Err(e) => {
                                    eprintln!(
                                        "Error writing {}/v{}/{}: {}",
                                        entry.component, entry.version, key, e
                                    );
                                    skipped += 1;
                                }
                            }
                        }
                    }
                    (Operation::Read, EntryContent::ReadDeleteEntries(keys)) => {
                        for key in keys {
                            match read_configuration(&entry.component, &entry.version, &key) {
                                Ok(content) => {
                                    println!(
                                        "{}/v{}/{}: {}",
                                        entry.component, entry.version, key, content
                                    );
                                    read_count += 1;
                                }
                                Err(e) => {
                                    if *verbose {
                                        println!(
                                            "Entry not found: {}/v{}/{}: {}",
                                            entry.component, entry.version, key, e
                                        );
                                    }
                                    skipped += 1;
                                }
                            }
                        }
                    }
                    (Operation::Delete, EntryContent::ReadDeleteEntries(keys)) => {
                        for key in keys {
                            match delete_configuration(&entry.component, &entry.version, &key) {
                                Ok(()) => {
                                    if *verbose {
                                        println!(
                                            "Deleted: {}/v{}/{}",
                                            entry.component, entry.version, key
                                        );
                                    }
                                    delete_count += 1;
                                }
                                Err(e) => {
                                    if *verbose {
                                        println!(
                                            "Failed to delete {}/v{}/{}: {}",
                                            entry.component, entry.version, key, e
                                        );
                                    }
                                    skipped += 1;
                                }
                            }
                        }
                    }
                    _ => {
                        eprintln!("Invalid combination of operation and entries format");
                        return;
                    }
                }
            }

            println!(
                "Operations completed successfully. {} writes, {} reads, {} deletes, {} entries skipped.",
                write_changes, read_count, delete_count, skipped
            );
        }
        Commands::Backup { file, verbose } => {
            let cosmic_path = get_cosmic_configurations();
            let mut operations: HashMap<(String, u64), HashMap<String, String>> = HashMap::new();
            let mut entry_count = 0;

            for entry in WalkDir::new(cosmic_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
            {
                if let Some((component, version, entry_name)) =
                    parse_configuration_path(entry.path())
                {
                    match read_configuration(&component, &version, &entry_name) {
                        Ok(content) => {
                            if *verbose {
                                println!("Backing up: {}/v{}/{}", component, version, entry_name);
                            }

                            operations
                                .entry((component.clone(), version))
                                .or_default()
                                .insert(entry_name, content);

                            entry_count += 1;
                        }
                        Err(e) => {
                            if *verbose {
                                println!(
                                    "Failed to backup {}/v{}/{}: {}",
                                    component, version, entry_name, e
                                );
                            }
                        }
                    }
                }
            }

            let backup_data = ConfigFile {
                schema: "https://raw.githubusercontent.com/HeitorAugustoLN/cosmic-ctl/refs/heads/main/schema.json".to_string(),
                operations: operations
                    .into_iter()
                    .map(|((component, version), entries)| Entry {
                        component,
                        version,
                        operation: Operation::Write,
                        entries: EntryContent::WriteEntries(entries),
                })
                .collect(),
            };

            let json_data = serde_json::to_string_pretty(&backup_data)
                .expect("Failed to serialize backup data");

            fs::write(file, json_data).expect("Unable to write backup file");
            println!(
                "Backup completed successfully. {} entries backed up.",
                entry_count
            );
        }
        Commands::Reset {
            force,
            verbose,
            exclude,
        } => {
            if !*force {
                print!("Are you sure you want to delete all configuration entries? This action cannot be undone. [y/N] ");
                std::io::stdout().flush().unwrap();

                let mut response = String::new();
                std::io::stdin().read_line(&mut response).unwrap();

                if !response.trim().eq_ignore_ascii_case("y") {
                    println!("Operation cancelled.");
                    return;
                }
            }

            let cosmic_path = get_cosmic_configurations();
            let mut deleted_count = 0;
            let mut errors = Vec::new();

            if !cosmic_path.exists() {
                println!("No configurations to delete.");
                return;
            }

            let exclude_patterns = split_string_respect_braces(exclude.clone())
                .into_iter()
                .flat_map(|pattern| explode(&pattern).unwrap_or_else(|_| vec![pattern.clone()]))
                .filter_map(|pattern| {
                    let pattern = if !pattern.contains('/') {
                        format!("{}/**", pattern)
                    } else if pattern.matches('/').count() == 1 {
                        format!("{}/*", pattern)
                    } else {
                        pattern
                    };

                    match Pattern::new(&pattern) {
                        Ok(p) => Some(p),
                        Err(e) => {
                            errors.push(format!("Invalid pattern '{}': {}", pattern, e));
                            None
                        }
                    }
                })
                .collect::<Vec<_>>();

            for entry in WalkDir::new(&cosmic_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
            {
                if let Some((component, version, entry_name)) =
                    parse_configuration_path(entry.path())
                {
                    let relative_path = format!("{}/v{}/{}", component, version, entry_name);
                    let should_exclude = exclude_patterns
                        .iter()
                        .any(|pattern| pattern.matches(&relative_path));

                    if should_exclude {
                        if *verbose {
                            println!("Skipping excluded path: {}", relative_path);
                        }
                        continue;
                    }

                    if *verbose {
                        println!("Deleting: {}", entry.path().display());
                    }

                    match delete_configuration(&component, &version, &entry_name) {
                        Ok(()) => deleted_count += 1,
                        Err(e) => errors.push(format!("{}: {}", entry.path().display(), e)),
                    }
                }
            }

            if errors.is_empty() {
                println!(
                    "Successfully deleted {} configuration entries.",
                    deleted_count
                );
            } else {
                println!(
                    "Deleted {} configuration entries with {} errors:",
                    deleted_count,
                    errors.len()
                );
                for error in errors {
                    eprintln!("Error: {}", error);
                }
            }
        }
    }
}

fn read_configuration(component: &str, version: &u64, entry: &str) -> Result<String, Error> {
    let path = get_configuration_path(component, version, entry);

    if path.exists() {
        fs::read_to_string(path)
    } else {
        Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "Configuration entry not found: {}/v{}/{}",
                component, version, entry
            ),
        ))
    }
}

fn write_configuration(
    component: &str,
    version: &u64,
    entry: &str,
    value: &str,
) -> Result<bool, Error> {
    let path = get_configuration_path(component, version, entry);
    let unescaped_value = unescape(value).map_err(|e| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("Failed to unescape value: {}", e),
        )
    })?;

    if let Ok(current_value) = read_configuration(component, version, entry) {
        if current_value == unescaped_value {
            return Ok(false);
        }
    }

    fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(""))).map_err(|e| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to create directory structure: {}", e),
        )
    })?;
    fs::write(&path, unescaped_value).map_err(|e| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to write configuration to {}: {}", path.display(), e),
        )
    })?;

    Ok(true)
}

fn delete_configuration(component: &str, version: &u64, entry: &str) -> Result<(), Error> {
    let path = get_configuration_path(component, version, entry);
    if path.exists() {
        fs::remove_file(path)?;
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::NotFound,
            "Configuration entry does not exist",
        ))
    }
}

fn parse_configuration_path(path: &Path) -> Option<(String, u64, String)> {
    let parts: Vec<_> = path.iter().collect();

    if parts.len() < 4 {
        return None;
    }

    let entry_name = parts.last()?.to_str()?.to_string();
    let version_str = parts.get(parts.len() - 2)?.to_str()?;
    let version = version_str.strip_prefix('v')?.parse().ok()?;
    let component = parts.get(parts.len() - 3)?.to_str()?.to_string();

    Some((component, version, entry_name))
}

fn get_configuration_path(component: &str, version: &u64, entry: &str) -> PathBuf {
    let cosmic_folder = get_cosmic_configurations();

    Path::new(&cosmic_folder)
        .join(component)
        .join(format!("v{}", version))
        .join(entry)
}

fn get_cosmic_configurations() -> PathBuf {
    let config_home = env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
        let home = env::var("HOME").unwrap();
        format!("{}/.config", home)
    });

    Path::new(&config_home).join("cosmic")
}

fn split_string_respect_braces(input_string: Option<String>) -> Vec<String> {
    match input_string {
        None => Vec::new(),
        Some(string) => {
            let mut result = Vec::new();
            let mut current_string = String::new();
            let mut brace_count = 0;

            for character in string.chars() {
                match character {
                    '{' => {
                        brace_count += 1;
                        current_string.push(character);
                    }
                    '}' => {
                        brace_count -= 1;
                        current_string.push(character);
                    }
                    ',' if brace_count == 0 => {
                        if !current_string.is_empty() {
                            result.push(current_string.trim().to_string());
                            current_string = String::new();
                        }
                    }
                    _ => current_string.push(character),
                }
            }

            if !current_string.is_empty() {
                result.push(current_string.trim().to_string());
            }

            result
        }
    }
}
