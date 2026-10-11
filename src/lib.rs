use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

pub type Result<T> = std::result::Result<T, String>;
pub fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn new_id() -> Result<String> {
    let mut bytes = [0u8; 12];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub fn valid_id(id: &str) -> bool {
    id.len() == 24 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub folder: PathBuf,
    pub kind: String,
    pub launch: String,
    pub build: String,
    pub docs: Option<PathBuf>,
    pub terminal: bool,
    pub favorite: bool,
    pub created: u64,
}
impl Project {
    pub fn validate(&self) -> Result<()> {
        if !valid_id(&self.id) {
            return Err("Invalid project ID".into());
        }
        if self.name.trim().is_empty() || self.name.len() > 160 {
            return Err("Give the project a name (up to 160 characters).".into());
        }
        if !self.folder.is_dir() {
            return Err("The source folder does not exist.".into());
        }
        parse_command(&self.launch)?;
        if !self.build.trim().is_empty() {
            parse_command(&self.build)?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Registry {
    pub schema_version: u32,
    pub revision: u64,
    pub projects: Vec<Project>,
}
impl Default for Registry {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            projects: Vec::new(),
        }
    }
}
#[derive(Clone)]
pub struct Store {
    pub root: PathBuf,
}
impl Store {
    pub fn default_location() -> Result<Self> {
        let root = if let Some(path) = std::env::var_os("LAUNCHSHELF_DATA") {
            PathBuf::from(path)
        } else if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
            PathBuf::from(path).join("launchshelf")
        } else {
            PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unavailable")?)
                .join(".local/share/launchshelf")
        };
        Ok(Self { root })
    }
    pub fn load(&self) -> Result<Registry> {
        let path = self.root.join("projects.json");
        if !path.exists() {
            return Ok(Registry::default());
        }
        let state: Registry = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| {
            format!("Cannot read project library; existing data was preserved: {e}")
        })?;
        if state.schema_version != 1 {
            return Err("Unsupported library version; existing data was preserved.".into());
        }
        let mut ids = std::collections::HashSet::new();
        if state
            .projects
            .iter()
            .any(|p| !valid_id(&p.id) || !ids.insert(p.id.clone()))
        {
            return Err("Library contains invalid or duplicate IDs; data was preserved.".into());
        }
        Ok(state)
    }
    pub fn save(&self, state: &mut Registry) -> Result<()> {
        fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("registry.lock"))
            .map_err(|e| e.to_string())?;
        lock.lock().map_err(|e| e.to_string())?;
        let existing = self.load()?;
        if existing.revision != state.revision {
            return Err("The library changed in another window. Reload before saving.".into());
        }
        let mut next = state.clone();
        next.revision += 1;
        let path = self.root.join("projects.json");
        if path.exists() {
            fs::copy(&path, self.root.join("projects.previous.json")).map_err(|e| e.to_string())?;
        }
        let temporary = self.root.join("projects.tmp");
        let mut file = fs::File::create(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(&next).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(temporary, path).map_err(|e| e.to_string())?;
        *state = next;
        Ok(())
    }
}
pub fn parse_command(text: &str) -> Result<Vec<String>> {
    let parts = shlex::split(text).ok_or("Command has an unmatched quote.")?;
    if parts.is_empty() || parts[0].is_empty() {
        return Err("Enter a launch command, including its executable.".into());
    }
    if parts.iter().any(|s| s.contains('\0')) {
        return Err("Commands cannot contain NUL bytes.".into());
    }
    if parts
        .iter()
        .any(|s| ["&&", "||", ";", "|", ">", ">>", "<", "&"].contains(&s.as_str()))
    {
        return Err("Shell operators need an explicit shell, for example: bash -lc 'command one && command two'.".into());
    }
    if parts[0].contains('=') && !parts[0].contains('/') {
        return Err("For environment variables, use: env NAME=value command".into());
    }
    Ok(parts)
}
pub fn quote(text: &str) -> String {
    shlex::try_quote(text)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| "''".into())
}
fn read_small(path: &Path) -> Option<String> {
    let f = fs::File::open(path).ok()?;
    let mut text = String::new();
    f.take(1024 * 1024).read_to_string(&mut text).ok()?;
    Some(text)
}
pub fn detect(folder: &Path) -> Result<Project> {
    let folder = folder
        .canonicalize()
        .map_err(|e| format!("Cannot open source folder: {e}"))?;
    if !folder.is_dir() {
        return Err("Drop a project folder, not an individual source file.".into());
    }
    let mut p = Project {
        id: new_id()?,
        name: folder
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        folder: folder.clone(),
        kind: "Custom".into(),
        launch: String::new(),
        build: String::new(),
        docs: None,
        terminal: false,
        favorite: false,
        created: timestamp(),
    };
    for file in ["README.md", "README", "docs/index.md"] {
        if folder.join(file).is_file() {
            p.docs = Some(folder.join(file));
            break;
        }
    }
    if let Some(text) = read_small(&folder.join("Cargo.toml")) {
        if let Ok(manifest) = toml::from_str::<toml::Value>(&text) {
            p.kind = "Rust".into();
            p.launch = "cargo run --release".into();
            p.build = "cargo build --release".into();
            if let Some(name) = manifest
                .get("package")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
            {
                p.name = name.to_owned()
            }
        }
    } else if let Some(text) = read_small(&folder.join("package.json")) {
        if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) {
            p.kind = "JavaScript".into();
            if let Some(name) = manifest["name"].as_str() {
                p.name = name.into()
            }
            let manager = if folder.join("pnpm-lock.yaml").is_file() {
                "pnpm"
            } else if folder.join("yarn.lock").is_file() {
                "yarn"
            } else {
                "npm"
            };
            if manifest["scripts"]["start"].is_string() {
                p.launch = format!("{manager} run start")
            } else if manifest["scripts"]["dev"].is_string() {
                p.launch = format!("{manager} run dev")
            }
            if manifest["scripts"]["build"].is_string() {
                p.build = format!("{manager} run build")
            }
        }
    } else {
        for name in ["app.py", "main.py"] {
            if folder.join(name).is_file() {
                p.kind = "Python".into();
                p.launch = format!("python3 {name}");
                break;
            }
        }
        if p.launch.is_empty() && folder.join("play").is_file() {
            p.kind = "Script".into();
            p.launch = "./play".into();
            p.terminal = true
        }
    }
    Ok(p)
}
pub fn desktop_import(path: &Path) -> Result<Project> {
    if path.extension().and_then(|s| s.to_str()) != Some("desktop") {
        return Err("Choose an installed .desktop entry.".into());
    }
    let text = read_small(path).ok_or("Cannot read desktop entry")?;
    let mut section = false;
    let mut values = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = line == "[Desktop Entry]";
            continue;
        }
        if section
            && !line.starts_with('#')
            && let Some((k, v)) = line.split_once('=')
        {
            values.insert(k.trim(), v.trim());
        }
    }
    if values.get("Type") != Some(&"Application") {
        return Err("The file is not an application desktop entry.".into());
    }
    let name = values
        .get("Name")
        .ok_or("Desktop entry has no name")?
        .to_string();
    let folder = values
        .get("Path")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()));
    let desktop_id = path
        .file_name()
        .ok_or("Invalid desktop entry path")?
        .to_string_lossy();
    let docs = [folder.join("README.md"), folder.join("README")]
        .into_iter()
        .find(|p| p.is_file());
    Ok(Project {
        id: new_id()?,
        name,
        folder,
        kind: "Desktop app".into(),
        launch: format!("gtk-launch {}", quote(&desktop_id)),
        build: String::new(),
        docs,
        terminal: false,
        favorite: false,
        created: timestamp(),
    })
}
#[derive(Clone, Debug)]
pub struct GitInfo {
    pub branch: String,
    pub changes: usize,
    pub error: Option<String>,
}
pub fn git_info(folder: &Path) -> Option<GitInfo> {
    let output = Command::new("git")
        .args(["-c", "core.fsmonitor=false"])
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(folder)
        .args(["status", "--porcelain=v1", "--branch", "-z"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut records = text.split('\0').filter(|r| !r.is_empty());
    let first = records.next()?;
    let branch = first
        .strip_prefix("## ")
        .unwrap_or(first)
        .split("...")
        .next()
        .unwrap_or(first)
        .trim()
        .to_owned();
    let mut changes = 0;
    let mut skip_next = false;
    for entry in records {
        if skip_next {
            skip_next = false;
            continue;
        }
        changes += 1;
        if entry.starts_with('R')
            || entry.starts_with(" R")
            || entry.starts_with('C')
            || entry.starts_with(" C")
        {
            skip_next = true
        }
    }
    Some(GitInfo {
        branch,
        changes,
        error: None,
    })
}
#[derive(Clone, Debug)]
pub struct Job {
    pub status: String,
    pub log: PathBuf,
    pub done: bool,
    pub success: bool,
}
pub type SharedJob = Arc<Mutex<Job>>;
pub fn run(store: &Store, project: &Project, build: bool) -> Result<SharedJob> {
    if !project.folder.is_dir() {
        return Err(format!(
            "Source folder is missing: {}",
            project.folder.display()
        ));
    }
    let command = if build {
        &project.build
    } else {
        &project.launch
    };
    let parts = parse_command(command)?;
    let dir = store.root.join("logs").join(&project.id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}-{}.log", timestamp(), new_id()?));
    let mut log = fs::File::create(&path).map_err(|e| e.to_string())?;
    writeln!(
        log,
        "{} command: {}\nWorking directory: {}\n",
        if build { "Build" } else { "Launch" },
        command,
        project.folder.display()
    )
    .map_err(|e| e.to_string())?;
    let mut process = if project.terminal && !build {
        let terminal=["foot","kitty","alacritty","xterm"].into_iter().find(|p|executable_exists(p)).ok_or("No supported terminal found. Install foot, kitty, alacritty or xterm, or disable Run in terminal.")?;
        let mut c = Command::new(terminal);
        c.arg("-e").args(&parts);
        c
    } else {
        let mut c = Command::new(&parts[0]);
        c.args(&parts[1..]);
        c
    };
    let stderr = log.try_clone().map_err(|e| e.to_string())?;
    let child = process
        .current_dir(&project.folder)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(e) => {
            let message = format!(
                "Could not start {}: {e}. Check the executable and working folder.",
                parts[0]
            );
            let mut log = OpenOptions::new()
                .append(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            let _ = writeln!(log, "{message}");
            return Err(message);
        }
    };
    let shared = Arc::new(Mutex::new(Job {
        status: format!(
            "{} running · PID {}",
            if build { "Build" } else { "Launch" },
            child.id()
        ),
        log: path.clone(),
        done: false,
        success: false,
    }));
    let monitor = shared.clone();
    thread::spawn(move || {
        let result = child.wait();
        let (mut status, success) = match result {
            Ok(s) if s.success() => (
                format!(
                    "{} command exited successfully",
                    if build { "Build" } else { "Launch" }
                ),
                true,
            ),
            Ok(s) => (
                format!("{} failed: {s}", if build { "Build" } else { "Launch" }),
                false,
            ),
            Err(e) => (format!("Unable to monitor process: {e}"), false),
        };
        if let Ok(mut log) = OpenOptions::new().append(true).open(path) {
            let _ = writeln!(log, "\n{status}");
        }
        if !success {
            status.push_str(" · inspect the log");
        }
        let mut state = monitor.lock().unwrap();
        state.status = status;
        state.success = success;
        state.done = true;
    });
    Ok(shared)
}
pub fn executable_exists(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let valid = |path: &Path| {
        fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if name.contains('/') {
        return valid(Path::new(name));
    }
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| valid(&dir.join(name))))
}
pub fn read_log(path: &Path) -> String {
    let Ok(mut file) = fs::File::open(path) else {
        return "No log is available yet.".into();
    };
    use std::io::{Seek, SeekFrom};
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if len > 128 * 1024 {
        let _ = file.seek(SeekFrom::End(-128 * 1024));
    }
    let mut bytes = Vec::new();
    let _ = file.take(128 * 1024).read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}
fn desktop_quote(text: &str) -> String {
    format!(
        "\"{}\"",
        text.replace('\\', "\\\\\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$")
            .replace('%', "%%")
    )
}
pub fn desktop_contents(executable: &Path, project: &Project) -> Result<String> {
    if !valid_id(&project.id) || !executable.is_absolute() {
        return Err("Invalid desktop registration parameters".into());
    }
    let name = project.name.replace(['\n', '\r'], " ");
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName={name}\nComment=Managed by LaunchShelf\nExec={} --launch {}\nIcon=launchshelf\nTerminal=false\nCategories=Development;\nKeywords=LaunchShelf;local;project;\nX-LaunchShelf-Project={}\n",
        desktop_quote(&executable.to_string_lossy()),
        project.id,
        project.id
    ))
}
pub fn register_desktop(executable: &Path, project: &Project) -> Result<PathBuf> {
    let root = if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(path)
    } else {
        PathBuf::from(std::env::var_os("HOME").ok_or("HOME unavailable")?).join(".local/share")
    };
    let dir = root.join("applications");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("launchshelf-{}.desktop", project.id));
    if path.exists() {
        let old = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if !old.contains(&format!("X-LaunchShelf-Project={}\n", project.id)) {
            return Err("Existing launcher is not owned by LaunchShelf.".into());
        }
    }
    fs::write(&path, desktop_contents(executable, project)?).map_err(|e| e.to_string())?;
    Ok(path)
}
pub fn unregister_desktop(project: &Project) -> Result<()> {
    let root = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
        });
    let path = root
        .join("applications")
        .join(format!("launchshelf-{}.desktop", project.id));
    if !path.exists() {
        return Ok(());
    }
    let contents = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if !contents.contains(&format!("X-LaunchShelf-Project={}\n", project.id)) {
        return Err("Launcher ownership check failed; file was preserved.".into());
    }
    fs::remove_file(path).map_err(|e| e.to_string())
}
