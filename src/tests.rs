use super::*;
use std::fs;
use tempfile::TempDir;

/// Cross-platform command that always fails with exit code 1
#[cfg(windows)]
const FAIL_CMD: &str = "cmd /c exit 1";
#[cfg(not(windows))]
const FAIL_CMD: &str = "false";

/// Returns a cross-platform touch command for the given path
#[cfg(windows)]
fn touch_cmd(path: &std::path::Path) -> String {
    // Simple echo redirect works with cmd /c
    // The > creates the file if it doesn't exist
    format!("echo.>\"{}\"", path.display())
}
#[cfg(not(windows))]
fn touch_cmd(path: &std::path::Path) -> String {
    format!("touch {}", path.display())
}

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
        directories: vec![
            dir1.to_str().unwrap().to_string(),
            dir2.to_str().unwrap().to_string(),
        ],
        ignore: vec![],
        verbose: false,
        silent: true,
        add_aliases_to_global_looprc: false,
        include_filters: None,
        exclude_filters: None,
        parallel: false,
        dry_run: false,
        json_output: false,
        spawn_stagger_ms: 0,
        env: None,
        max_parallel: None,
        root_dir: None,
    };

    let result = run(&config, "echo test");
    assert!(result.is_ok());

    // Test with a failing command
    let result = run(&config, FAIL_CMD);
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

    let result =
        execute_command_in_directory(temp_dir.path(), "echo test", &config, &aliases, None);
    assert!(result.success);
    assert_eq!(result.exit_code, 0);

    let result = execute_command_in_directory(temp_dir.path(), FAIL_CMD, &config, &aliases, None);
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
        dry_run: false,
        json_output: false,
        spawn_stagger_ms: 0,
        env: None,
        max_parallel: None,
        root_dir: None,
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
        dry_run: false,
        json_output: false,
        spawn_stagger_ms: 0,
        env: None,
        max_parallel: None,
        root_dir: None,
    };

    let result = run(&config, "echo test");
    assert!(result.is_ok());

    // Test with a failing command in parallel mode
    let result = run(&config, FAIL_CMD);
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

    let result = execute_command_in_directory_capturing(
        temp_dir.path(),
        "echo hello",
        &config,
        &aliases,
        None,
    );
    assert!(result.success);
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("hello"));

    let result =
        execute_command_in_directory_capturing(temp_dir.path(), FAIL_CMD, &config, &aliases, None);
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
    let result = execute_command_in_directory_capturing(
        temp_dir.path(),
        "echo error >&2",
        &config,
        &aliases,
        None,
    );
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

    let result =
        execute_command_in_directory_capturing(nonexistent, "echo test", &config, &aliases, None);
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
        dry_run: false,
        json_output: false,
        spawn_stagger_ms: 0,
        env: None,
        max_parallel: None,
        root_dir: None,
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
        dry_run: false,
        json_output: false,
        spawn_stagger_ms: 0,
        env: None,
        max_parallel: None,
        root_dir: None,
    };

    let result = run(&config, "echo test");
    assert!(result.is_ok());
}

// ============================================================================
// Tests for new dry_run and json_output functionality
// ============================================================================

#[test]
fn test_loop_config_default_includes_new_fields() {
    let config = LoopConfig::default();
    assert!(!config.dry_run);
    assert!(!config.json_output);
    assert!(!config.parallel);
}

#[test]
fn test_dry_run_does_not_execute() {
    let temp_dir = TempDir::new().unwrap();
    let marker_file = temp_dir.path().join("marker.txt");

    let config = LoopConfig {
        directories: vec![temp_dir.path().to_str().unwrap().to_string()],
        dry_run: true,
        silent: true,
        ..Default::default()
    };

    // This command would create a file if executed
    let cmd = touch_cmd(&marker_file);
    let result = run(&config, &cmd);
    assert!(result.is_ok());

    // File should NOT exist because dry_run is true
    assert!(!marker_file.exists(), "dry_run should not execute commands");
}

#[test]
fn test_dry_run_returns_success() {
    let temp_dir = TempDir::new().unwrap();

    let config = LoopConfig {
        directories: vec![temp_dir.path().to_str().unwrap().to_string()],
        dry_run: true,
        silent: true,
        ..Default::default()
    };

    // Even a command that would fail should succeed in dry_run mode
    let result = run(&config, FAIL_CMD);
    assert!(result.is_ok(), "dry_run should always succeed");
}

#[test]
fn test_execute_command_in_directory_dry_run() {
    let config = LoopConfig {
        dry_run: true,
        silent: true,
        ..Default::default()
    };
    let aliases = HashMap::new();
    let temp_dir = TempDir::new().unwrap();

    let result = execute_command_in_directory(temp_dir.path(), FAIL_CMD, &config, &aliases, None);
    assert!(result.success, "dry_run should return success");
    assert_eq!(result.exit_code, 0);
}

#[test]
fn test_execute_command_in_directory_capturing_dry_run() {
    let config = LoopConfig {
        dry_run: true,
        silent: true,
        ..Default::default()
    };
    let aliases = HashMap::new();
    let temp_dir = TempDir::new().unwrap();

    let result = execute_command_in_directory_capturing(
        temp_dir.path(),
        "echo hello",
        &config,
        &aliases,
        None,
    );
    assert!(result.success);
    assert!(result.stdout.contains("[DRY RUN]"));
    assert!(result.stdout.contains("echo hello"));
}

#[test]
fn test_dir_command_struct() {
    let cmd = DirCommand {
        dir: "/some/path".to_string(),
        cmd: "git status".to_string(),
        env: None,
    };
    assert_eq!(cmd.dir, "/some/path");
    assert_eq!(cmd.cmd, "git status");
}

#[test]
fn test_run_commands_empty_list() {
    let config = LoopConfig::default();
    let commands: Vec<DirCommand> = vec![];
    let result = run_commands(&config, &commands);
    assert!(result.is_ok());
}

#[test]
fn test_run_commands_sequential() {
    let temp_dir = TempDir::new().unwrap();
    let dir1 = temp_dir.path().join("dir1");
    let dir2 = temp_dir.path().join("dir2");
    fs::create_dir(&dir1).unwrap();
    fs::create_dir(&dir2).unwrap();

    let config = LoopConfig {
        parallel: false,
        silent: true,
        ..Default::default()
    };

    let commands = vec![
        DirCommand {
            dir: dir1.to_str().unwrap().to_string(),
            cmd: "echo test1".to_string(),
            env: None,
        },
        DirCommand {
            dir: dir2.to_str().unwrap().to_string(),
            cmd: "echo test2".to_string(),
            env: None,
        },
    ];

    let result = run_commands(&config, &commands);
    assert!(result.is_ok());
}

#[test]
fn test_run_commands_parallel() {
    let temp_dir = TempDir::new().unwrap();
    let dir1 = temp_dir.path().join("dir1");
    let dir2 = temp_dir.path().join("dir2");
    fs::create_dir(&dir1).unwrap();
    fs::create_dir(&dir2).unwrap();

    let config = LoopConfig {
        parallel: true,
        silent: true,
        ..Default::default()
    };

    let commands = vec![
        DirCommand {
            dir: dir1.to_str().unwrap().to_string(),
            cmd: "echo test1".to_string(),
            env: None,
        },
        DirCommand {
            dir: dir2.to_str().unwrap().to_string(),
            cmd: "echo test2".to_string(),
            env: None,
        },
    ];

    let result = run_commands(&config, &commands);
    assert!(result.is_ok());
}

#[test]
fn test_run_commands_with_different_commands() {
    let temp_dir = TempDir::new().unwrap();
    let dir1 = temp_dir.path().join("dir1");
    let dir2 = temp_dir.path().join("dir2");
    fs::create_dir(&dir1).unwrap();
    fs::create_dir(&dir2).unwrap();

    let config = LoopConfig {
        parallel: false,
        silent: true,
        ..Default::default()
    };

    // Use simple echo commands with different arguments to verify
    // that different commands can be executed in different directories
    let commands = vec![
        DirCommand {
            dir: dir1.to_str().unwrap().to_string(),
            cmd: "echo command1".to_string(),
            env: None,
        },
        DirCommand {
            dir: dir2.to_str().unwrap().to_string(),
            cmd: "echo command2".to_string(),
            env: None,
        },
    ];

    let result = run_commands(&config, &commands);
    assert!(
        result.is_ok(),
        "Different commands should execute in different directories"
    );
}

#[test]
fn test_run_commands_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let marker_file = temp_dir.path().join("marker.txt");

    let config = LoopConfig {
        dry_run: true,
        silent: true,
        ..Default::default()
    };

    let commands = vec![DirCommand {
        dir: temp_dir.path().to_str().unwrap().to_string(),
        cmd: touch_cmd(&marker_file),
        env: None,
    }];

    let result = run_commands(&config, &commands);
    assert!(result.is_ok());
    assert!(!marker_file.exists(), "dry_run should not execute commands");
}

#[test]
fn test_run_commands_failure_handling() {
    let temp_dir = TempDir::new().unwrap();
    let dir1 = temp_dir.path().join("dir1");
    fs::create_dir(&dir1).unwrap();

    let config = LoopConfig {
        parallel: false,
        silent: true,
        ..Default::default()
    };

    let commands = vec![DirCommand {
        dir: dir1.to_str().unwrap().to_string(),
        cmd: FAIL_CMD.to_string(), // This command always fails
        env: None,
    }];

    let result = run_commands(&config, &commands);
    assert!(result.is_err(), "Should return error when command fails");
}

#[test]
fn test_run_commands_failure_in_dry_run_succeeds() {
    let temp_dir = TempDir::new().unwrap();

    let config = LoopConfig {
        dry_run: true,
        silent: true,
        ..Default::default()
    };

    let commands = vec![DirCommand {
        dir: temp_dir.path().to_str().unwrap().to_string(),
        cmd: FAIL_CMD.to_string(),
        env: None,
    }];

    let result = run_commands(&config, &commands);
    assert!(
        result.is_ok(),
        "dry_run should succeed even for failing commands"
    );
}

#[test]
fn test_json_output_structures() {
    // Test JsonOutput serialization
    let output = JsonOutput {
        success: true,
        results: vec![JsonCommandResult {
            directory: "/test".to_string(),
            command: "echo hello".to_string(),
            success: true,
            exit_code: 0,
            stdout: "hello\n".to_string(),
            stderr: String::new(),
        }],
        summary: JsonSummary {
            total: 1,
            succeeded: 1,
            failed: 0,
            dry_run: false,
        },
    };

    let json = serde_json::to_string(&output).unwrap();
    assert!(json.contains("\"success\":true"));
    assert!(json.contains("\"directory\":\"/test\""));
    assert!(json.contains("\"total\":1"));
}

#[test]
fn test_json_output_skips_empty_strings() {
    let result = JsonCommandResult {
        directory: "/test".to_string(),
        command: "echo".to_string(),
        success: true,
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
    };

    let json = serde_json::to_string(&result).unwrap();
    // Empty stdout/stderr should be skipped due to skip_serializing_if
    assert!(!json.contains("\"stdout\":\"\""));
    assert!(!json.contains("\"stderr\":\"\""));
}

#[test]
fn test_loop_config_serialization() {
    let config = LoopConfig {
        directories: vec!["dir1".to_string()],
        dry_run: true,
        json_output: true,
        parallel: true,
        ..Default::default()
    };

    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"dry_run\":true"));
    assert!(json.contains("\"json_output\":true"));
    assert!(json.contains("\"parallel\":true"));
}

#[test]
fn test_loop_config_deserialization_with_new_fields() {
    let json = r#"{
        "directories": ["test"],
        "dry_run": true,
        "json_output": true,
        "parallel": false
    }"#;

    let config: LoopConfig = serde_json::from_str(json).unwrap();
    assert!(config.dry_run);
    assert!(config.json_output);
    assert!(!config.parallel);
}

#[test]
fn test_loop_config_deserialization_missing_new_fields() {
    // Old config format without new fields should deserialize with defaults
    let json = r#"{
        "directories": ["test"],
        "verbose": false
    }"#;

    let config: LoopConfig = serde_json::from_str(json).unwrap();
    assert!(!config.dry_run, "dry_run should default to false");
    assert!(!config.json_output, "json_output should default to false");
    assert!(!config.parallel, "parallel should default to false");
}

#[test]
fn test_dir_command_serialization() {
    let cmd = DirCommand {
        dir: "/path/to/dir".to_string(),
        cmd: "git status".to_string(),
        env: None,
    };

    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains("\"dir\":\"/path/to/dir\""));
    assert!(json.contains("\"cmd\":\"git status\""));

    // Test deserialization
    let parsed: DirCommand = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.dir, "/path/to/dir");
    assert_eq!(parsed.cmd, "git status");
}
