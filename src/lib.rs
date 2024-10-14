use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Deserialize)]
pub struct LoopConfig {
    #[serde(default)]
    pub directories: Vec<String>,
    #[serde(default)]
    pub ignore: Vec<String>,
}

impl Default for LoopConfig {
    fn default() -> Self {
        LoopConfig {
            directories: vec![],
            ignore: vec![".git".to_string()],
        }
    }
}

pub fn run(config: &LoopConfig, command: &str) -> Result<()> {
    for dir in &config.directories {
        if !config.ignore.iter().any(|ignored| dir.contains(ignored)) {
            println!("Executing in directory: {}", dir);
            let output = Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(dir)
                .output()
                .with_context(|| format!("Failed to execute command in directory: {}", dir))?;

            println!("Status: {}", output.status);
            println!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
            println!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
    Ok(())
}

pub fn parse_config(config_path: &Path) -> Result<LoopConfig> {
    let config_str = fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
    let config: LoopConfig = serde_json::from_str(&config_str)
        .with_context(|| format!("Failed to parse config file: {:?}", config_path))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_parse_config() -> Result<()> {
        let dir = tempdir()?;
        let config_path = dir.path().join(".looprc");
        let mut file = File::create(&config_path)?;
        writeln!(file, r#"{{""ignore": [".git"]}}"#)?;

        let config = parse_config(&config_path)?;
        assert_eq!(config.directories, vec!["dir1", "dir2"]);
        assert_eq!(config.ignore, vec![".git"]);
        Ok(())
    }

    #[test]
    fn test_default_config() -> Result<()> {
        let dir = tempdir()?;
        let config_path = dir.path().join(".looprc");
        let mut file = File::create(&config_path)?;
        writeln!(file, r#"{{}}"#)?;

        let config = parse_config(&config_path)?;
        assert_eq!(config.directories, Vec::<String>::new());
        assert_eq!(config.ignore, vec![".git"]);
        Ok(())
    }
}
