use super::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_parse_config() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(".looprc");
    let config_content = r#"
    {
        "directories": ["dir1", "dir2"],
        "ignore": [".git"],
        "verbose": true,
        "silent": false,
        "add_aliases_to_global_looprc": false
    }
    "#;
    fs::write(&config_path, config_content).unwrap();

    let config = parse_config(&config_path).unwrap();
    assert_eq!(config.directories, vec!["dir1", "dir2"]);
    assert_eq!(config.ignore, vec![".git"]);
    assert!(config.verbose);
    assert!(!config.silent);
    assert!(!config.add_aliases_to_global_looprc);
}

#[test]
fn test_expand_directories() {
    let temp_dir = TempDir::new().unwrap();
    let dir1 = temp_dir.path().join("dir1");
    let dir2 = temp_dir.path().join("dir2");
    let subdir = dir1.join("subdir");
    fs::create_dir_all(&subdir).unwrap();

    fs::create_dir(&dir2).unwrap();

    let directories = vec![temp_dir.path().to_str().unwrap().to_string()];
    let ignore = vec![".git".to_string()];

    let expanded = crate::expand_directories(&directories, &ignore).unwrap();
    
    assert_eq!(expanded.len(), 3); // Including the root directory itself
    assert!(expanded.contains(&temp_dir.path().to_str().unwrap().to_string()));
    assert!(expanded.contains(&dir1.to_str().unwrap().to_string()));
    assert!(expanded.contains(&dir2.to_str().unwrap().to_string()));
    assert!(!expanded.contains(&subdir.to_str().unwrap().to_string())); // Ensure subdirectories are not included
}

#[test]
fn test_should_ignore() {
    let path = Path::new("/some/path/.git/file");
    let ignore = vec![".git".to_string()];
    assert!(should_ignore(path, &ignore));

    let path = Path::new("/some/path/normal/file");
    assert!(!should_ignore(path, &ignore));
}

#[test]
fn test_get_aliases() {
    let temp_dir = TempDir::new().unwrap();
    let looprc_path = temp_dir.path().join(".looprc");
    let looprc_content = r#"
    {
        "aliases": {
            "ll": "ls -l",
            "gst": "git status"
        }
    }
    "#;
    fs::write(&looprc_path, looprc_content).unwrap();

    // Temporarily set HOME to our temp directory
    std::env::set_var("HOME", temp_dir.path());

    let aliases = get_aliases();
    assert_eq!(aliases.get("ll"), Some(&"ls -l".to_string()));
    assert_eq!(aliases.get("gst"), Some(&"git status".to_string()));

    // Reset HOME
    std::env::remove_var("HOME");
}

#[test]
fn test_run() {
    let temp_dir = TempDir::new().unwrap();
    let dir1 = temp_dir.path().join("dir1");
    let dir2 = temp_dir.path().join("dir2");
    fs::create_dir(&dir1).unwrap();
    fs::create_dir(&dir2).unwrap();

    let config = LoopConfig {
        directories: vec![dir1.to_str().unwrap().to_string(), dir2.to_str().unwrap().to_string()],
        ignore: vec![],
        verbose: false,
        silent: true,
        add_aliases_to_global_looprc: false,
        include_filters: None,
        exclude_filters: None,
        parallel: false,
    };

    let result = run(&config, "echo test");
    assert!(result.is_ok());

    // Test with a failing command
    let result = run(&config, "false");
    assert!(result.is_err()); // The function should return an error if any command fails
}

#[test]
fn test_load_aliases_from_file() {
    let temp_dir = TempDir::new().unwrap();
    let looprc_path = temp_dir.path().join(".looprc");
    let looprc_content = r#"
    {
        "aliases": {
            "ll": "ls -l",
            "gst": "git status"
        }
    }
    "#;
    fs::write(&looprc_path, looprc_content).unwrap();

    let aliases = load_aliases_from_file(&looprc_path).unwrap();
    assert_eq!(aliases.get("ll"), Some(&"ls -l".to_string()));
    assert_eq!(aliases.get("gst"), Some(&"git status".to_string()));
}

#[test]
fn test_execute_command_in_directory() {
    let config = LoopConfig {
        verbose: false,
        silent: true,
        ..Default::default()
    };
    let aliases = HashMap::new();
    let temp_dir = TempDir::new().unwrap();

    let result = execute_command_in_directory(temp_dir.path(), "echo test", &config, &aliases);
    assert!(result.success);
    assert_eq!(result.exit_code, 0);

    let result = execute_command_in_directory(temp_dir.path(), "false", &config, &aliases);
    assert!(!result.success);
    assert_eq!(result.exit_code, 1);
}
#[test]
fn test_run_without_looprc() {
    let temp_dir = TempDir::new().unwrap();
    let dir1 = temp_dir.path().join("dir1");
    let dir2 = temp_dir.path().join("dir2");
    fs::create_dir(&dir1).unwrap();
    fs::create_dir(&dir2).unwrap();

    // Run without a .looprc file
    let config = LoopConfig {
        directories: vec![temp_dir.path().to_str().unwrap().to_string()],
        ignore: vec![],
        verbose: false,
        silent: true,
        add_aliases_to_global_looprc: false,
        include_filters: None,
        exclude_filters: None,
        parallel: false,
    };

    let result = run(&config, "echo test");
    assert!(result.is_ok());
}

#[test]
fn test_run_parallel() {
    let temp_dir = TempDir::new().unwrap();
    let dir1 = temp_dir.path().join("dir1");
    let dir2 = temp_dir.path().join("dir2");
    let dir3 = temp_dir.path().join("dir3");
    fs::create_dir(&dir1).unwrap();
    fs::create_dir(&dir2).unwrap();
    fs::create_dir(&dir3).unwrap();

    let config = LoopConfig {
        directories: vec![
            dir1.to_str().unwrap().to_string(),
            dir2.to_str().unwrap().to_string(),
            dir3.to_str().unwrap().to_string(),
        ],
        ignore: vec![],
        verbose: false,
        silent: true,
        add_aliases_to_global_looprc: false,
        include_filters: None,
        exclude_filters: None,
        parallel: true,
    };

    let result = run(&config, "echo test");
    assert!(result.is_ok());

    // Test with a failing command in parallel mode
    let result = run(&config, "false");
    assert!(result.is_err());
}

#[test]
fn test_execute_command_in_directory_capturing() {
    let config = LoopConfig {
        verbose: false,
        silent: true,
        ..Default::default()
    };
    let aliases = HashMap::new();
    let temp_dir = TempDir::new().unwrap();

    let result = execute_command_in_directory_capturing(temp_dir.path(), "echo hello", &config, &aliases);
    assert!(result.success);
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("hello"));

    let result = execute_command_in_directory_capturing(temp_dir.path(), "false", &config, &aliases);
    assert!(!result.success);
    assert_eq!(result.exit_code, 1);
}

#[test]
fn test_execute_command_in_directory_capturing_stderr() {
    let config = LoopConfig {
        verbose: false,
        silent: true,
        ..Default::default()
    };
    let aliases = HashMap::new();
    let temp_dir = TempDir::new().unwrap();

    // Command that writes to stderr
    let result = execute_command_in_directory_capturing(temp_dir.path(), "echo error >&2", &config, &aliases);
    assert!(result.success);
    assert!(result.stderr.contains("error"));
}

#[test]
fn test_execute_command_nonexistent_directory() {
    let config = LoopConfig {
        verbose: false,
        silent: true,
        ..Default::default()
    };
    let aliases = HashMap::new();
    let nonexistent = Path::new("/nonexistent/path/that/does/not/exist");

    let result = execute_command_in_directory_capturing(nonexistent, "echo test", &config, &aliases);
    assert!(!result.success);
    assert_eq!(result.exit_code, 1);
    assert!(result.stderr.contains("does not exist"));
}

#[test]
fn test_command_result_default() {
    let result = CommandResult::default();
    assert!(!result.success);
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.is_empty());
    assert!(result.stderr.is_empty());
}

#[test]
fn test_include_filters() {
    let temp_dir = TempDir::new().unwrap();
    let dir1 = temp_dir.path().join("project_a");
    let dir2 = temp_dir.path().join("project_b");
    let dir3 = temp_dir.path().join("other");
    fs::create_dir(&dir1).unwrap();
    fs::create_dir(&dir2).unwrap();
    fs::create_dir(&dir3).unwrap();

    let config = LoopConfig {
        directories: vec![
            dir1.to_str().unwrap().to_string(),
            dir2.to_str().unwrap().to_string(),
            dir3.to_str().unwrap().to_string(),
        ],
        ignore: vec![],
        verbose: false,
        silent: true,
        add_aliases_to_global_looprc: false,
        include_filters: Some(vec!["project".to_string()]),
        exclude_filters: None,
        parallel: false,
    };

    // The run function should only execute on directories matching the filter
    let result = run(&config, "echo test");
    assert!(result.is_ok());
}

#[test]
fn test_exclude_filters() {
    let temp_dir = TempDir::new().unwrap();
    let dir1 = temp_dir.path().join("project_a");
    let dir2 = temp_dir.path().join("project_b");
    let dir3 = temp_dir.path().join("excluded");
    fs::create_dir(&dir1).unwrap();
    fs::create_dir(&dir2).unwrap();
    fs::create_dir(&dir3).unwrap();

    let config = LoopConfig {
        directories: vec![
            dir1.to_str().unwrap().to_string(),
            dir2.to_str().unwrap().to_string(),
            dir3.to_str().unwrap().to_string(),
        ],
        ignore: vec![],
        verbose: false,
        silent: true,
        add_aliases_to_global_looprc: false,
        include_filters: None,
        exclude_filters: Some(vec!["excluded".to_string()]),
        parallel: false,
    };

    let result = run(&config, "echo test");
    assert!(result.is_ok());
}
