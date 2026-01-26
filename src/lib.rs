use anyhow::{Context, Result};
use colored::*;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    pub add_aliases_to_global_looprc: bool,
    #[serde(default)]
    pub include_filters: Option<Vec<String>>,
    #[serde(default)]
    pub exclude_filters: Option<Vec<String>>,
    #[serde(default)]
    pub parallel: bool,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub json_output: bool,
    /// Milliseconds to wait between spawning threads in parallel mode.
    /// Default is 0 (no stagger). Set to e.g. 10 to spread out connections.
    #[serde(default)]
    pub spawn_stagger_ms: u64,
}

/// A command to execute in a specific directory
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DirCommand {
    pub dir: String,
    pub cmd: String,
}

impl Default for LoopConfig {
    fn default() -> Self {
        LoopConfig {
            directories: vec![],
            ignore: vec![".git".to_string()],
            verbose: false,
            silent: false,
            add_aliases_to_global_looprc: false,
            include_filters: None,
            exclude_filters: None,
            parallel: false,
            dry_run: false,
            json_output: false,
            spawn_stagger_ms: 0,
        }
    }
}

#[derive(Default)]
pub struct CommandResult {
    pub success: bool,
    pub exit_code: i32,
    pub directory: PathBuf,
    pub command: String,
    pub stdout: String,
    pub stderr: String,
}

pub fn load_aliases_from_file(path: &Path) -> Result<HashMap<String, String>> {
    let content = fs::read_to_string(path)?;
    let config: serde_json::Value = serde_json::from_str(&content)?;
    let aliases = config["aliases"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("No 'aliases' object found in config file"))?;
    Ok(aliases
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
        .collect())
}

fn prompt_user(question: &str) -> Result<bool> {
    print!("{question} [y/N]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_lowercase() == "y")
}

pub fn add_aliases_to_global_looprc() -> Result<()> {
    println!("Starting add_aliases_to_global_looprc function");

    let home = env::var("HOME").context("Failed to get HOME directory")?;
    let global_looprc = PathBuf::from(home).join(".looprc");
    println!("Global .looprc path: {global_looprc:?}");

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
    println!("Using shell: {shell}");

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
            let command = command
                .trim()
                .trim_matches('\'')
                .trim_matches('"')
                .to_string();
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
                diff::Result::Left(l) => println!("{}", format!("-{l}").red()),
                diff::Result::Both(l, _) => println!(" {l}"),
                diff::Result::Right(r) => println!("{}", format!("+{r}").green()),
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

pub fn execute_command_in_directory(
    dir: &Path,
    command: &str,
    config: &LoopConfig,
    aliases: &HashMap<String, String>,
) -> CommandResult {
    if !dir.exists() {
        println!("\nNo directory found for {}", dir.display());
        let dir_name = dir.file_name().unwrap_or_default().to_str().unwrap();
        println!(
            "\x1b[31m\n✗ {}: No directory found. Command: {} (Exit code: {})\x1b[0m",
            dir_name, command, 1
        );
        return CommandResult {
            success: false,
            exit_code: 1,
            directory: dir.to_path_buf(),
            command: command.to_string(),
            stdout: String::new(),
            stderr: String::new(),
        };
    }

    // Resolve aliases for display
    let resolved_command = command
        .split_whitespace()
        .next()
        .and_then(|cmd| aliases.get(cmd).map(|alias_cmd| (cmd, alias_cmd)))
        .map(|(cmd, alias_cmd)| command.replacen(cmd, alias_cmd, 1))
        .unwrap_or_else(|| command.to_string());

    // Dry run mode: print what would be executed without running it
    if config.dry_run {
        let dir_display = if dir.as_os_str() == "." {
            if let Ok(cwd) = std::env::current_dir() {
                cwd.display().to_string()
            } else {
                ".".to_string()
            }
        } else {
            dir.display().to_string()
        };
        println!(
            "{} Would execute in {}:\n  {}",
            "[DRY RUN]".cyan(),
            dir_display.yellow(),
            resolved_command
        );
        return CommandResult {
            success: true,
            exit_code: 0,
            directory: dir.to_path_buf(),
            command: resolved_command,
            stdout: String::new(),
            stderr: String::new(),
        };
    }

    if config.verbose {
        println!("Executing in directory: {}", dir.display());
    }

    if !config.silent {
        println!();
        io::stdout().flush().unwrap();
    }

    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    let mut child = Command::new(&shell)
        .arg("-c")
        .arg(&resolved_command)
        .current_dir(dir)
        .envs(env::vars())
        .stdout(if config.silent {
            Stdio::null()
        } else {
            Stdio::inherit()
        })
        .stderr(if config.silent {
            Stdio::null()
        } else {
            Stdio::inherit()
        })
        .spawn()
        .with_context(|| {
            format!(
                "Failed to execute command '{}' in directory '{}'",
                resolved_command,
                dir.display()
            )
        })
        .expect("Failed to execute command");

    let status = child.wait().expect("Failed to wait on child process");
    let exit_code = status.code().unwrap_or(-1);
    let success = status.success();

    if !config.silent {
        let dir_name = dir
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|&s| !s.is_empty())
            .unwrap_or(".");
        if success {
            if dir_name == "." {
                if let Ok(cwd) = std::env::current_dir() {
                    if let Some(base) = cwd.file_name().and_then(|s| s.to_str()) {
                        println!("\x1b[32m\n✓ . ({base})\x1b[0m");
                    } else {
                        println!("\x1b[32m\n✓ .\x1b[0m");
                    }
                } else {
                    println!("\x1b[32m\n✓ .\x1b[0m");
                }
            } else {
                println!("\x1b[32m\n✓ {dir_name}\x1b[0m");
            }
        } else {
            println!("\x1b[31m\n✗ {dir_name}: exited code {exit_code}\x1b[0m");
        }
        io::stdout().flush().unwrap();
    }

    CommandResult {
        success,
        exit_code,
        directory: dir.to_path_buf(),
        command: resolved_command,
        stdout: String::new(), // Sequential mode uses Stdio::inherit(), so no capture
        stderr: String::new(),
    }
}

/// Capturing version for parallel execution - captures stdout/stderr for display after completion
pub fn execute_command_in_directory_capturing(
    dir: &Path,
    command: &str,
    config: &LoopConfig,
    aliases: &HashMap<String, String>,
) -> CommandResult {
    if !dir.exists() {
        return CommandResult {
            success: false,
            exit_code: 1,
            directory: dir.to_path_buf(),
            command: command.to_string(),
            stdout: String::new(),
            stderr: format!("Directory does not exist: {}", dir.display()),
        };
    }

    let resolved_command = command
        .split_whitespace()
        .next()
        .and_then(|cmd| aliases.get(cmd).map(|alias_cmd| (cmd, alias_cmd)))
        .map(|(cmd, alias_cmd)| command.replacen(cmd, alias_cmd, 1))
        .unwrap_or_else(|| command.to_string());

    // Dry run mode: return what would be executed without running it
    if config.dry_run {
        let dir_display = if dir.as_os_str() == "." {
            if let Ok(cwd) = std::env::current_dir() {
                cwd.display().to_string()
            } else {
                ".".to_string()
            }
        } else {
            dir.display().to_string()
        };
        let stdout_msg = format!("[DRY RUN] Would execute in {dir_display}:\n  {resolved_command}");
        return CommandResult {
            success: true,
            exit_code: 0,
            directory: dir.to_path_buf(),
            command: resolved_command,
            stdout: stdout_msg,
            stderr: String::new(),
        };
    }

    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    let output = Command::new(&shell)
        .arg("-c")
        .arg(&resolved_command)
        .current_dir(dir)
        .envs(env::vars())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(output) => {
            let success = output.status.success();
            let exit_code = output.status.code().unwrap_or(-1);
            CommandResult {
                success,
                exit_code,
                directory: dir.to_path_buf(),
                command: resolved_command,
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            }
        }
        Err(e) => CommandResult {
            success: false,
            exit_code: -1,
            directory: dir.to_path_buf(),
            command: resolved_command,
            stdout: String::new(),
            stderr: format!("Failed to execute: {e}"),
        },
    }
}

pub fn expand_directories(directories: &[String], ignore: &[String]) -> Result<Vec<String>> {
    let mut expanded = Vec::new();

    use std::fs;

    for dir in directories {
        let dir_path = PathBuf::from(dir);
        if dir_path.is_dir() && !should_ignore(&dir_path, ignore) {
            expanded.push(dir_path.to_string_lossy().into_owned());

            for entry in fs::read_dir(&dir_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() && !should_ignore(&path, ignore) {
                    expanded.push(path.to_string_lossy().into_owned());
                }
            }
        }
    }

    Ok(expanded)
}

/// Run the same command across multiple directories.
/// This applies include/exclude filters and then delegates to the unified execution engine.
pub fn run(orig_config: &LoopConfig, command: &str) -> Result<()> {
    // Handle special case: add_aliases_to_global_looprc
    if orig_config.add_aliases_to_global_looprc {
        return add_aliases_to_global_looprc();
    }

    // Apply include/exclude filters to directories
    let mut dirs = orig_config.directories.clone();

    if let Some(ref includes) = orig_config.include_filters {
        if !includes.is_empty() {
            dirs.retain(|p| includes.iter().any(|f| p.contains(f)));
        }
    }

    if let Some(ref excludes) = orig_config.exclude_filters {
        if !excludes.is_empty() {
            if orig_config.verbose {
                println!("Exclude filters: {excludes:?}");
            }
            dirs.retain(|p| {
                let excluded = excludes.iter().any(|f| {
                    let f = f.trim_end_matches('/');
                    p.contains(f)
                });
                if orig_config.verbose {
                    println!("Dir: {p}, excluded: {excluded}");
                }
                !excluded
            });
        }
    }

    // Build DirCommand list with same command for each directory
    let commands: Vec<DirCommand> = dirs
        .iter()
        .map(|dir| DirCommand {
            dir: dir.clone(),
            cmd: command.to_string(),
        })
        .collect();

    // Delegate to unified execution engine
    execute_commands_internal(orig_config, &commands)
}

/// JSON output structure for command results
#[derive(Debug, Serialize)]
pub struct JsonOutput {
    pub success: bool,
    pub results: Vec<JsonCommandResult>,
    pub summary: JsonSummary,
}

#[derive(Debug, Serialize)]
pub struct JsonCommandResult {
    pub directory: String,
    pub command: String,
    pub success: bool,
    pub exit_code: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub stdout: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub stderr: String,
}

#[derive(Debug, Serialize)]
pub struct JsonSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub dry_run: bool,
}

// ============================================================================
// Unified Execution Engine
// ============================================================================

/// Internal execution engine that handles both parallel and sequential execution.
/// This is the unified implementation used by both `run()` and `run_commands()`.
fn execute_commands_internal(config: &LoopConfig, commands: &[DirCommand]) -> Result<()> {
    if commands.is_empty() {
        return Ok(());
    }

    let results = Arc::new(Mutex::new(Vec::new()));
    let aliases = Arc::new(get_aliases());

    if config.parallel {
        // Parallel execution using rayon thread pool with spinners
        let is_tty = std::io::stdout().is_terminal() && !config.json_output;
        let mp = if is_tty {
            Some(Arc::new(MultiProgress::new()))
        } else {
            None
        };
        let spinner_style = ProgressStyle::with_template("{prefix:.bold.dim} {spinner} {wide_msg}")
            .unwrap()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ");

        let total = commands.len();

        // Pre-create all spinners (in order) so they display in correct sequence
        let spinners: Vec<Option<ProgressBar>> = commands
            .iter()
            .enumerate()
            .map(|(i, dir_cmd)| {
                if let Some(ref mp) = mp {
                    let pb = mp.add(ProgressBar::new_spinner());
                    pb.set_style(spinner_style.clone());
                    pb.set_prefix(format!("[{}/{}]", i + 1, total));
                    let dir_name = PathBuf::from(&dir_cmd.dir)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(".")
                        .to_string();
                    pb.set_message(format!("{dir_name}: pending..."));
                    pb.enable_steady_tick(Duration::from_millis(100));
                    Some(pb)
                } else {
                    None
                }
            })
            .collect();

        // Use rayon's parallel iterator for bounded thread pool execution
        let parallel_results: Vec<CommandResult> = commands
            .par_iter()
            .enumerate()
            .map(|(i, dir_cmd)| {
                let dir = PathBuf::from(&dir_cmd.dir);
                let dir_name = dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(".")
                    .to_string();
                let prefix = format!("[{}/{}]", i + 1, total);

                // Update spinner to show running
                if let Some(ref pb) = spinners[i] {
                    pb.set_message(format!("{dir_name}: running..."));
                }

                let result =
                    execute_command_in_directory_capturing(&dir, &dir_cmd.cmd, config, &aliases);

                // Update spinner with result (only if not JSON output)
                if !config.json_output {
                    if let Some(ref pb) = spinners[i] {
                        if result.success {
                            pb.finish_with_message(format!("{} {}", "✓".green(), dir_name.green()));
                        } else {
                            pb.finish_with_message(format!(
                                "{} {} (exit {})",
                                "✗".red(),
                                dir_name.red(),
                                result.exit_code
                            ));
                        }
                    } else {
                        // Non-TTY output
                        if result.success {
                            println!("{} {} {}", prefix, "✓".green(), dir_name.green());
                        } else {
                            println!(
                                "{} {} {} (exit {})",
                                prefix,
                                "✗".red(),
                                dir_name.red(),
                                result.exit_code
                            );
                        }
                    }
                }

                result
            })
            .collect();

        // Store results (already collected from rayon)
        results.lock().unwrap_or_else(|e| e.into_inner()).extend(parallel_results);

        // Print captured output after all spinners complete (if not JSON)
        if !config.silent && !config.json_output {
            let results = results.lock().unwrap_or_else(|e| e.into_inner());
            let has_any_output = results
                .iter()
                .any(|r| !r.stdout.trim().is_empty() || !r.stderr.trim().is_empty());

            if has_any_output {
                // Two newlines: one to ensure we're past the spinner area, one for the blank line
                println!("\n");
            }

            for result in results.iter() {
                let dir_name = result
                    .directory
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(".");

                let has_output =
                    !result.stdout.trim().is_empty() || !result.stderr.trim().is_empty();
                if has_output {
                    if result.success {
                        println!("{} {}:", "✓".green(), dir_name.green());
                    } else {
                        println!("{} {}:", "✗".red(), dir_name.red());
                    }
                    if !result.stdout.trim().is_empty() {
                        print!("{}", result.stdout);
                    }
                    if !result.stderr.trim().is_empty() {
                        print!("{}", result.stderr);
                    }
                    println!(); // Blank line after each repo's output
                }
            }
        }
    } else {
        // Sequential execution
        for dir_cmd in commands {
            let dir = PathBuf::from(&dir_cmd.dir);
            let result = if config.json_output {
                // Capture output for JSON mode
                execute_command_in_directory_capturing(&dir, &dir_cmd.cmd, config, &aliases)
            } else {
                execute_command_in_directory(&dir, &dir_cmd.cmd, config, &aliases)
            };
            results.lock().unwrap_or_else(|e| e.into_inner()).push(result);
        }
    }

    // Build results summary
    let results = results.lock().unwrap_or_else(|e| e.into_inner());
    let total = results.len();
    let failed: Vec<_> = results.iter().filter(|r| !r.success).collect();
    let failed_count = failed.len();

    // Output results
    if config.json_output {
        // JSON output mode
        let json_results: Vec<JsonCommandResult> = results
            .iter()
            .map(|r| JsonCommandResult {
                directory: r.directory.display().to_string(),
                command: r.command.clone(),
                success: r.success,
                exit_code: r.exit_code,
                stdout: r.stdout.clone(),
                stderr: r.stderr.clone(),
            })
            .collect();

        let output = JsonOutput {
            success: failed_count == 0,
            results: json_results,
            summary: JsonSummary {
                total,
                succeeded: total - failed_count,
                failed: failed_count,
                dry_run: config.dry_run,
            },
        };

        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if !config.silent {
        // Text output mode
        if config.dry_run {
            println!(
                "\n{} Would run {} command(s) across {} directories",
                "[DRY RUN]".cyan(),
                total.to_string().yellow(),
                total.to_string().yellow()
            );
        } else if failed_count == 0 {
            println!("{} commands complete", total.to_string().green());
        } else {
            println!(
                "\nSummary: {} {} out of {} commands failed",
                "✗".red(),
                failed_count.to_string().red(),
                total
            );
            for result in &failed {
                println!(
                    "\n{} {}: {} (Exit code {}) ",
                    "✗".red(),
                    result.directory.display(),
                    result.command,
                    result.exit_code
                );
            }
            println!();
        }
    }

    if failed_count > 0 && !config.dry_run {
        return Err(anyhow::anyhow!("At least one command failed"));
    }

    Ok(())
}

/// Execute a list of commands (each with its own directory)
/// This is the unified execution engine for plugins.
/// Applies include/exclude filters from config before executing.
pub fn run_commands(config: &LoopConfig, commands: &[DirCommand]) -> Result<()> {
    let mut filtered: Vec<DirCommand> = commands.to_vec();

    if let Some(ref includes) = config.include_filters {
        if !includes.is_empty() {
            filtered.retain(|c| includes.iter().any(|f| c.dir.contains(f)));
        }
    }

    if let Some(ref excludes) = config.exclude_filters {
        if !excludes.is_empty() {
            filtered.retain(|c| {
                let excluded = excludes.iter().any(|f| {
                    let f = f.trim_end_matches('/');
                    c.dir.contains(f)
                });
                !excluded
            });
        }
    }

    execute_commands_internal(config, &filtered)
}

pub fn should_ignore(path: &Path, ignore: &[String]) -> bool {
    ignore.iter().any(|i| path.to_string_lossy().contains(i))
}

pub fn parse_config(config_path: &Path) -> Result<LoopConfig> {
    let config_str = fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read looprc config file: {config_path:?}"))?;
    let config: LoopConfig = serde_json::from_str(&config_str)
        .with_context(|| format!("Failed to parse looprc config file: {config_path:?}"))?;
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
                    let command = command
                        .trim()
                        .trim_matches('\'')
                        .trim_matches('"')
                        .to_string();
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
#[path = "tests.rs"]
mod tests;
