#[cfg(test)]
mod tests;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env, fs,
    io::Write,
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
        version: u32,
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
        version: u32,
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
        version: u32,
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
    },
    /// Delete all configuration entries
    Reset {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
        /// Show which entries are being deleted
        #[arg(short, long)]
        verbose: bool,
    },
}

#[derive(Deserialize, Serialize)]
struct Entry {
    component: String,
    version: u32,
    entries: HashMap<String, String>,
}

#[derive(Deserialize, Serialize)]
struct ConfigFile {
    #[serde(rename = "$schema")]
    schema: String,
    configurations: Vec<Entry>,
}

fn main() {
    let cli: Cli = Cli::parse();

    match &cli.command {
        Commands::Write {
            component,
            version,
            entry,
            value,
        } => {
            if !apply_configuration(component, version, entry, value) {
                println!("Doing nothing, entry already has this value.");
            } else {
                println!("Configuration entry written successfully.");
            }
        }
        Commands::Read {
            version,
            component,
            entry,
        } => {
            let path = get_config_path(component, version, entry);

            if path.exists() {
                let contents = fs::read_to_string(path).unwrap();
                println!("{}", contents);
            } else {
                eprintln!("Error: Configuration entry does not exist.");
            }
        }
        Commands::Delete {
            version,
            component,
            entry,
        } => {
            let path = get_config_path(component, version, entry);

            if path.exists() {
                fs::remove_file(path).unwrap();
                println!("Configuration entry deleted successfully.");
            } else {
                eprintln!("Error: Configuration entry does not exist.");
            }
        }
        Commands::Apply { file, verbose } => {
            if file.extension().and_then(|s| s.to_str()) != Some("json") {
                eprintln!("Error: The file is not in JSON format.");
                return;
            }

            let file_content = fs::read_to_string(file).expect("Unable to read file");
            let config_file: ConfigFile =
                serde_json::from_str(&file_content).expect("Invalid JSON format");

            let mut changes = 0;
            let mut skipped = 0;

            for entry in config_file.configurations {
                for (key, value) in entry.entries {
                    if !apply_configuration(&entry.component, &entry.version, &key, &value) {
                        if *verbose {
                            println!(
                                "Skipping {}/v{}/{} - value unchanged",
                                entry.component, entry.version, key
                            );
                        }
                        skipped += 1;
                    } else {
                        changes += 1;
                    }
                }
            }

            println!(
                "Configurations applied successfully. {} changes made, {} entries skipped.",
                changes, skipped
            );
        }
        Commands::Backup { file } => {
            let backup_data = create_backup();
            let json_data = serde_json::to_string_pretty(&backup_data)
                .expect("Failed to serialize backup data");

            fs::write(file, json_data).expect("Unable to write backup file");
            println!("Backup completed successfully.");
        }
        Commands::Reset { force, verbose } => {
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

            let (deleted_count, errors) = delete_all_configurations(*verbose);

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

fn check_existing_value(path: &Path, new_value: &str) -> bool {
    if path.exists() {
        if let Ok(current_value) = fs::read_to_string(path) {
            return current_value == new_value;
        }
    }

    false
}

fn apply_configuration(component: &str, version: &u32, entry: &str, value: &str) -> bool {
    let path = get_config_path(component, version, entry);
    let unescaped_value = unescape(value).unwrap();

    if check_existing_value(&path, &unescaped_value) {
        return false;
    }

    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, unescaped_value).unwrap();

    true
}

fn create_backup() -> ConfigFile {
    let cosmic_path = get_cosmic_configs();
    let mut configurations: HashMap<(String, u32), HashMap<String, String>> = HashMap::new();

    for entry in WalkDir::new(cosmic_path).into_iter().filter_map(|e| e.ok()) {
        if entry.path().is_file() {
            if let Some((component, version, entry_name)) = parse_path(entry.path()) {
                let content = fs::read_to_string(entry.path()).unwrap();

                configurations
                    .entry((component.clone(), version))
                    .or_insert_with(HashMap::new)
                    .insert(entry_name, content);
            }
        }
    }

    ConfigFile {
        schema: "https://raw.githubusercontent.com/HeitorAugustoLN/cosmic-ctl/refs/heads/main/schema.json".to_string(),
        configurations: configurations
            .into_iter()
            .map(|((component, version), entries)| Entry {
                component,
                version,
                entries,
            })
            .collect(),
    }
}

fn delete_all_configurations(verbose: bool) -> (usize, Vec<String>) {
    let cosmic_path = get_cosmic_configs();
    let mut deleted_count = 0;
    let mut errors = Vec::new();

    if !cosmic_path.exists() {
        return (0, errors);
    }

    for entry in WalkDir::new(&cosmic_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        if verbose {
            println!("Deleting: {}", entry.path().display());
        }

        if let Err(e) = fs::remove_file(entry.path()) {
            errors.push(format!("{}: {}", entry.path().display(), e));
        } else {
            deleted_count += 1;
        }
    }

    (deleted_count, errors)
}

fn parse_path(path: &Path) -> Option<(String, u32, String)> {
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

fn get_config_path(component: &str, version: &u32, entry: &str) -> PathBuf {
    let cosmic_folder = get_cosmic_configs();

    Path::new(&cosmic_folder)
        .join(component)
        .join(format!("v{}", version))
        .join(entry)
}

fn get_cosmic_configs() -> PathBuf {
    let config_home = get_config_home();

    Path::new(&config_home).join("cosmic")
}

fn get_config_home() -> String {
    env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
        let home = env::var("HOME").unwrap();
        format!("{}/.config", home)
    })
}
