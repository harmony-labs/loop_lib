use anyhow::{Context, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::env;
use std::collections::HashMap;
use colored::*;
use diff;

#[derive(Debug, Deserialize, Serialize)]
pub struct LoopConfig {
    #[serde(default)]
    pub directories: Vec<String>,
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub silent: bool,
    #[serde(default)]
    pub parallel: bool,
    #[serde(default)]
    pub add_aliases_to_global_looprc: bool,
}

impl Default for LoopConfig {
    fn default() -> Self {
        LoopConfig {
            directories: vec![],
            ignore: vec![".git".to_string()],
            verbose: false,
            silent: false,
            parallel: false,
            add_aliases_to_global_looprc: false,
        }
    }
}

#[derive(Default)]
pub struct CommandResult {
    pub success: bool,
    pub exit_code: i32,
    pub directory: PathBuf,
    pub command: String,
}

pub fn load_aliases_from_file(path: &Path) -> Result<HashMap<String, String>> {
    let content = fs::read_to_string(path)?;
    let config: serde_json::Value = serde_json::from_str(&content)?;
    let aliases = config["aliases"].as_object()
        .ok_or_else(|| anyhow::anyhow!("No 'aliases' object found in config file"))?;
    Ok(aliases.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
}

fn prompt_user(question: &str) -> Result<bool> {
    print!("{} [y/N]: ", question);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_lowercase() == "y")
}

pub fn add_aliases_to_global_looprc() -> Result<()> {
    println!("Starting add_aliases_to_global_looprc function");
    
    let home = env::var("HOME").context("Failed to get HOME directory")?;
    let global_looprc = PathBuf::from(home).join(".looprc");
    println!("Global .looprc path: {:?}", global_looprc);

    let mut aliases = HashMap::new();
    let mut existing_content = String::new();

    if global_looprc.exists() {
        println!("Global .looprc exists, loading existing aliases");
        existing_content = fs::read_to_string(&global_looprc)?;
        aliases = load_aliases_from_file(&global_looprc)?;
    } else {
        println!("Global .looprc does not exist");
        if !prompt_user("The global .looprc file does not exist. Do you want to create it?")? {
            println!("Operation cancelled by user.");
            return Ok(());
        }
    }

    if !prompt_user("Do you want to set the value of the 'aliases' property?")? {
        println!("Operation cancelled by user.");
        return Ok(());
    }

    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    println!("Using shell: {}", shell);
    
    println!("Executing 'alias' command");
    let output = Command::new(&shell)
        .arg("-i")
        .arg("-c")
        .arg("alias")
        .output()?;

    println!("Processing 'alias' command output");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some((alias, command)) = line.split_once('=') {
            let alias = alias.trim().trim_start_matches("alias ").to_string();
            let command = command.trim().trim_matches('\'').trim_matches('"').to_string();
            aliases.insert(alias, command);
        }
    }

    println!("Creating config JSON");
    let config = serde_json::json!({
        "aliases": aliases
    });

    println!("Serializing config to string");
    let new_content = serde_json::to_string_pretty(&config)?;
    
    // Show preview of changes
    println!("\nPreview of changes:");
    if !existing_content.is_empty() {
        for diff in diff::lines(&existing_content, &new_content) {
            match diff {
                diff::Result::Left(l) => println!("{}", format!("-{}", l).red()),
                diff::Result::Both(l, _) => println!(" {}", l),
                diff::Result::Right(r) => println!("{}", format!("+{}", r).green()),
            }
        }
    } else {
        println!("{}", new_content.green());
    }

    if !prompt_user("Do you want to apply these changes?")? {
        println!("Operation cancelled by user.");
        return Ok(());
    }

    println!("Writing config to file");
    fs::write(global_looprc, new_content)?;

    println!("Aliases have been added to ~/.looprc");
    Ok(())
}

pub fn execute_command_in_directory(dir: &Path, command: &str, config: &LoopConfig, aliases: &HashMap<String, String>) -> CommandResult {
    if config.verbose {
        println!("Executing in directory: {}", dir.display());
    }

    if !config.silent {
        println!();
        io::stdout().flush().unwrap();
    }

    let command = command.split_whitespace().next()
        .and_then(|cmd| aliases.get(cmd).map(|alias_cmd| (cmd, alias_cmd)))
        .map(|(cmd, alias_cmd)| command.replacen(cmd, alias_cmd, 1))
        .unwrap_or_else(|| command.to_string());

    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    let mut child = Command::new(&shell)
        .arg("-c")
        .arg(&command)
        .current_dir(dir)
        .envs(env::vars())
        .stdout(if config.silent { Stdio::null() } else { Stdio::inherit() })
        .stderr(if config.silent { Stdio::null() } else { Stdio::inherit() })
        .spawn()
        .expect("Failed to execute command");

    let status = child.wait().expect("Failed to wait on child process");
    let exit_code = status.code().unwrap_or(-1);
    let success = status.success();

    if !config.silent {
        let dir_name = dir.file_name().unwrap_or_default().to_str().unwrap();
        if success {
            println!("\x1b[32m{} ✓\x1b[0m", dir_name);
        } else {
            println!("\x1b[31m{} ✗: exited code {}\x1b[0m", dir_name, exit_code);
        }
        io::stdout().flush().unwrap();
    }

    CommandResult {
        success,
        exit_code,
        directory: dir.to_path_buf(),
        command: command.to_string(),
    }
}

pub fn run(config: &LoopConfig, command: &str) -> Result<()> {
    if config.add_aliases_to_global_looprc {
        return add_aliases_to_global_looprc();
    }

    let dirs = expand_directories(&config.directories, &config.ignore)?;
    let results = Arc::new(Mutex::new(Vec::new()));
    let aliases = get_aliases();

    let run_command = |dir: &PathBuf| -> Result<()> {
        let result = execute_command_in_directory(dir, command, config, &aliases);
        results.lock().unwrap().push(result);
        Ok(())
    };

    if config.parallel {
        dirs.par_iter().try_for_each(run_command)?;
    } else {
        dirs.iter().try_for_each(run_command)?;
    }

    let results = results.lock().unwrap();
    let total = results.len();
    let failed: Vec<_> = results.iter().filter(|r| !r.success).collect();
    let failed_count = failed.len();

    if failed_count > 0 && !config.silent {
        println!("\nFailed commands:");
        for result in &failed {
            println!("{} in directory: {}", "✗".red(), result.directory.display());
            println!("Command: {}", result.command);
            println!("Exit code: {}", result.exit_code);
            println!();
        }
    }

    if !config.silent {
        println!("\nSummary:");
        if failed_count == 0 {
            println!("{} {} commands complete", "✓".green(), total.to_string().green());
        } else {
            println!("{} {} out of {} commands failed", "✗".red(), failed_count.to_string().red(), total);
        }
    }

    Ok(())
}

pub fn expand_directories(directories: &[String], ignore: &[String]) -> Result<Vec<PathBuf>> {
    let mut expanded = Vec::new();

    for dir in directories {
        let dir_path = PathBuf::from(dir);
        if dir_path.is_dir() && !should_ignore(&dir_path, ignore) {
            expanded.push(dir_path.clone());

            for entry in fs::read_dir(&dir_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() && !should_ignore(&path, ignore) {
                    expanded.push(path);
                }
            }
        }
    }

    Ok(expanded)
}

pub fn should_ignore(path: &Path, ignore: &[String]) -> bool {
    ignore.iter().any(|i| path.to_string_lossy().contains(i))
}

pub fn parse_config(config_path: &Path) -> Result<LoopConfig> {
    let config_str = fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
    let config: LoopConfig = serde_json::from_str(&config_str)
        .with_context(|| format!("Failed to parse config file: {:?}", config_path))?;
    Ok(config)
}

pub fn get_aliases() -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    
    if let Some(home) = env::var_os("HOME") {
        let global_looprc = PathBuf::from(home).join(".looprc");
        if global_looprc.exists() {
            if let Ok(global_aliases) = load_aliases_from_file(&global_looprc) {
                aliases.extend(global_aliases);
            }
        }
    }

    if aliases.is_empty() {
        if let Ok(output) = Command::new("sh").arg("-c").arg("alias").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some((alias, command)) = line.split_once('=') {
                    let alias = alias.trim().trim_start_matches("alias ").to_string();
                    let command = command.trim().trim_matches('\'').trim_matches('"').to_string();
                    aliases.insert(alias, command);
                }
            }
        }
    }

    if let Ok(local_aliases) = load_aliases_from_file(Path::new(".looprc")) {
        aliases.extend(local_aliases);
    }

    aliases
}

#[cfg(test)]
mod tests {
    include!("tests.rs");
}
