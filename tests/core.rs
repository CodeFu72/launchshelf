use launchshelf::{self as core, Registry, Store};
use std::{fs, path::PathBuf, thread, time::Duration};
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("launchshelf-test-{}", core::new_id().unwrap()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn quoted_commands_keep_spaces_and_do_not_expand_shell() {
    assert_eq!(
        core::parse_command("python3 'folder with spaces/app.py' '$HOME'").unwrap(),
        vec!["python3", "folder with spaces/app.py", "$HOME"]
    );
    assert!(core::parse_command("echo one && echo two").is_err());
    assert!(core::parse_command("echo 'unclosed").is_err());
    assert!(core::parse_command("TOKEN=secret app").is_err());
    assert_eq!(
        core::parse_command("bash -lc 'echo one && echo two'").unwrap()[2],
        "echo one && echo two"
    );
}
#[test]
fn cargo_project_detection_is_a_proposal_not_execution() {
    let t = Temp::new();
    fs::write(
        t.0.join("Cargo.toml"),
        "[package]\nname = 'orbit-notes'\nversion = '0.1.0'\n",
    )
    .unwrap();
    fs::write(t.0.join("README.md"), "# Notes").unwrap();
    let p = core::detect(&t.0).unwrap();
    assert_eq!(p.name, "orbit-notes");
    assert_eq!(p.kind, "Rust");
    assert_eq!(p.launch, "cargo run --release");
    assert!(p.docs.unwrap().is_file());
    assert!(!t.0.join("target").exists());
}
#[test]
fn javascript_detection_preserves_manager_and_custom_projects_need_review() {
    let t = Temp::new();
    fs::write(
        t.0.join("package.json"),
        r#"{"name":"signal-deck","scripts":{"dev":"vite","build":"vite build"}}"#,
    )
    .unwrap();
    fs::write(t.0.join("pnpm-lock.yaml"), "").unwrap();
    let p = core::detect(&t.0).unwrap();
    assert_eq!(p.launch, "pnpm run dev");
    assert_eq!(p.build, "pnpm run build");
    let empty = Temp::new();
    let p = core::detect(&empty.0).unwrap();
    assert!(p.launch.is_empty());
    assert!(p.validate().is_err());
}
#[test]
fn python_and_terminal_script_detection() {
    let t = Temp::new();
    fs::write(t.0.join("app.py"), "raise RuntimeError('not executed')").unwrap();
    let p = core::detect(&t.0).unwrap();
    assert_eq!(p.launch, "python3 app.py");
    assert!(!p.terminal);
    let script = Temp::new();
    fs::write(script.0.join("play"), "not a command").unwrap();
    let p = core::detect(&script.0).unwrap();
    assert_eq!(p.launch, "./play");
    assert!(p.terminal);
}
#[test]
fn registry_roundtrip_and_concurrent_window_protection() {
    let t = Temp::new();
    let store = Store {
        root: t.0.join("data"),
    };
    let mut state = Registry::default();
    let mut p = core::detect(&t.0).unwrap();
    p.launch = "printf hello".into();
    state.projects.push(p);
    store.save(&mut state).unwrap();
    assert_eq!(store.load().unwrap(), state);
    let mut stale = state.clone();
    state.projects[0].name = "Updated".into();
    store.save(&mut state).unwrap();
    assert!(store.save(&mut stale).is_err());
    assert_eq!(store.load().unwrap().projects[0].name, "Updated");
    assert!(store.root.join("projects.previous.json").exists());
}
#[test]
fn corrupt_or_newer_registry_is_never_overwritten() {
    let t = Temp::new();
    let store = Store { root: t.0.clone() };
    fs::write(t.0.join("projects.json"), "broken").unwrap();
    assert!(store.load().is_err());
    assert!(store.save(&mut Registry::default()).is_err());
    assert_eq!(
        fs::read_to_string(t.0.join("projects.json")).unwrap(),
        "broken"
    );
    fs::write(
        t.0.join("projects.json"),
        r#"{"schema_version":2,"revision":0,"projects":[]}"#,
    )
    .unwrap();
    assert!(store.load().is_err());
}
#[test]
fn desktop_import_delegates_exec_to_desktop_system() {
    let t = Temp::new();
    let path = t.0.join("A game.desktop");
    fs::write(&path,format!("[Desktop Entry]\nType=Application\nName=Pocket Game\nExec=some-tool %F\nPath={}\nTerminal=true\n",t.0.display())).unwrap();
    let p = core::desktop_import(&path).unwrap();
    assert_eq!(p.name, "Pocket Game");
    assert_eq!(p.folder, t.0);
    assert_eq!(
        core::parse_command(&p.launch).unwrap(),
        vec!["gtk-launch", "A game.desktop"]
    );
    assert!(!p.terminal);
}
#[test]
fn saved_launcher_points_to_registry_id_and_escapes_field_codes() {
    let t = Temp::new();
    let mut p = core::detect(&t.0).unwrap();
    p.name = "Name\nHidden=true".into();
    let contents = core::desktop_contents(
        std::path::Path::new("/tmp/apps with spaces/100%/launchshelf"),
        &p,
    )
    .unwrap();
    assert!(contents.contains("100%%"));
    assert!(contents.contains(&format!("--launch {}", p.id)));
    assert!(!contents.contains("\nHidden=true"));
    assert!(core::desktop_contents(std::path::Path::new("relative"), &p).is_err());
}
#[test]
fn launch_failure_retains_a_readable_log() {
    let t = Temp::new();
    let store = Store {
        root: t.0.join("data"),
    };
    let mut p = core::detect(&t.0).unwrap();
    p.launch = "/definitely/missing/launchshelf-tool".into();
    let error = core::run(&store, &p, false).err().unwrap();
    assert!(error.contains("Could not start"));
    let logs: Vec<_> = fs::read_dir(store.root.join("logs").join(&p.id))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(logs.len(), 1);
    assert!(core::read_log(&logs[0]).contains("Could not start"));
}
#[test]
fn process_exit_failure_and_success_are_reported() {
    let t = Temp::new();
    let store = Store {
        root: t.0.join("data"),
    };
    let mut p = core::detect(&t.0).unwrap();
    p.launch = "sh -c 'printf useful-error >&2; exit 7'".into();
    let job = core::run(&store, &p, false).unwrap();
    for _ in 0..100 {
        if job.lock().unwrap().done {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let result = job.lock().unwrap().clone();
    assert!(result.done && !result.success);
    assert!(result.status.contains('7'));
    assert!(core::read_log(&result.log).contains("useful-error"));
    p.launch = "printf 'hello world'".into();
    let job = core::run(&store, &p, false).unwrap();
    for _ in 0..100 {
        if job.lock().unwrap().done {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let result = job.lock().unwrap().clone();
    assert!(result.done && result.success);
    assert!(core::read_log(&result.log).contains("hello world"));
}

#[test]
fn git_status_counts_rename_as_one_change() {
    use std::process::Command;
    let t = Temp::new();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&t.0)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "-b", "main"]);
    fs::write(t.0.join("before.txt"), "fixture").unwrap();
    git(&["add", "before.txt"]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-m",
        "Fixture",
    ]);
    assert_eq!(core::git_info(&t.0).unwrap().changes, 0);
    git(&["mv", "before.txt", "after.txt"]);
    let info = core::git_info(&t.0).unwrap();
    assert_eq!(info.branch, "main");
    assert_eq!(info.changes, 1);
}
