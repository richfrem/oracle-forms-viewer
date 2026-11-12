//! Optional Track 1 bridge. Native parsing and runtime never depend on this toolchain.
use std::{collections::HashMap, fs, path::PathBuf, process::{Child, Command, Stdio}, sync::{Mutex, OnceLock, atomic::{AtomicU64, Ordering}}};

struct Job { child: Child, log: PathBuf }
static JOBS: OnceLock<Mutex<HashMap<String, Job>>> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(1);
fn jobs() -> &'static Mutex<HashMap<String, Job>> { JOBS.get_or_init(|| Mutex::new(HashMap::new())) }

pub fn start(filename: &str, credentials: Option<(String, String)>) -> Result<String, String> {
    if !filename.ends_with(".xml") || !filename.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.')) {
        return Err("Invalid saved form filename".into());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().map_err(|e| e.to_string())?;
    let script = root.join("plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py");
    if !root.join("oracle-reference-stack/oracle_home_14c/forms/java/frmsal.jar").is_file() {
        return Err("FSAL client is missing. Set up the Oracle 14c reference environment first.".into());
    }
    let mut jobs = jobs().lock().unwrap();
    let mut active = 0;
    jobs.retain(|_, job| match job.child.try_wait() { Ok(None) => { active += 1; true }, _ => true });
    if active >= 4 { return Err("Close an existing Oracle preview before opening another".into()); }
    if jobs.len() >= 32 {
        let completed: Vec<_> = jobs.iter_mut().filter_map(|(id, job)| job.child.try_wait().ok().flatten().map(|_| id.clone())).collect();
        for id in completed { if let Some(job) = jobs.remove(&id) { let _ = fs::remove_file(job.log); } }
    }
    let id = format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    // Oracle converters may choose output names from the internal module name.
    // Stage in an ignored job directory so conversion can never replace a source FMB.
    let staging = root.join("temp/oracle-preview").join(&id);
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let staged_xml = staging.join(filename);
    fs::copy(root.join("samples").join(filename), &staged_xml).map_err(|e| e.to_string())?;
    let log = std::env::temp_dir().join(format!("ofw-oracle-preview-{}.log", id));
    let output = fs::OpenOptions::new().write(true).create_new(true).open(&log).map_err(|e| e.to_string())?;
    let stderr = output.try_clone().map_err(|e| e.to_string())?;
    let mut command = Command::new("python3");
    command.arg("-u").arg(script).arg(staged_xml).args(["--interactive", "--userid", "none"]);
    if let Some((username, password)) = credentials {
        command.arg("--compile-from-env").env("OFW_PREVIEW_DB_LOGON", format!("{}/{}@FREEPDB1", username, password));
    } else { command.arg("--offline"); }
    let child = command.current_dir(&root).stdin(Stdio::null()).stdout(output).stderr(stderr).spawn().map_err(|e| e.to_string())?;
    jobs.insert(id.clone(), Job { child, log });
    Ok(id)
}

pub fn status(id: &str) -> Option<serde_json::Value> {
    let mut jobs = jobs().lock().unwrap();
    let job = jobs.get_mut(id)?;
    let exit = job.child.try_wait().ok().flatten();
    let bytes = fs::read(&job.log).unwrap_or_default();
    let log = String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(64000)..]);
    Some(serde_json::json!({ "running": exit.is_none(), "success": exit.map(|e| e.success()), "log": log }))
}

/// Read-only readiness probes. No container is started and no software is installed.
pub async fn readiness() -> serde_json::Value {
    async fn command(program: &str, args: &[&str]) -> bool {
        let mut command = tokio::process::Command::new(program);
        command.args(args).kill_on_drop(true).stdin(Stdio::null());
        match tokio::time::timeout(std::time::Duration::from_secs(4), command.output()).await {
            Ok(Ok(output)) if output.status.success() => {
                if program == "java" {
                    let version = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
                    version.split('"').nth(1).and_then(|value| value.split(['.', '-']).next())
                        .and_then(|major| major.parse::<u32>().ok()).is_some_and(|major| major >= 17)
                } else { true }
            }
            _ => false,
        }
    }
    async fn container(name: &str) -> bool {
        let mut command = tokio::process::Command::new("podman");
        command.args(["inspect", "--format", "{{.State.Running}}", name]).kill_on_drop(true).stdin(Stdio::null());
        match tokio::time::timeout(std::time::Duration::from_secs(4), command.output()).await {
            Ok(Ok(output)) => output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true",
            _ => false,
        }
    }
    async fn servlet() -> bool {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let probe = async {
            let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", 7001)).await?;
            stream.write_all(b"GET /forms/frmservlet HTTP/1.0\r\nHost: localhost:7001\r\n\r\n").await?;
            let mut buffer = [0; 256];
            let n = stream.read(&mut buffer).await?;
            let status = String::from_utf8_lossy(&buffer[..n]).split_whitespace().nth(1).unwrap_or("").to_string();
            Ok::<bool, std::io::Error>(matches!(status.as_str(), "200" | "301" | "302"))
        };
        matches!(tokio::time::timeout(std::time::Duration::from_secs(4), probe).await, Ok(Ok(true)))
    }
    let (python, java, podman, forms, database, listener) = tokio::join!(
        command("python3", &["--version"]), command("java", &["-version"]), command("podman", &["--version"]),
        container("forms-server-14c"), container("oracle23ai-lite"), servlet()
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jar = root.join("oracle-reference-stack/oracle_home_14c/forms/java/frmsal.jar").is_file();
    let tooling = root.join("plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py").is_file();
    serde_json::json!({ "ready": python && java && podman && forms && listener && jar && tooling,
        "checks": [
            { "label": "Python 3 on the server host", "ready": python, "required": true },
            { "label": "Java 17 or newer on the server host", "ready": java, "required": true },
            { "label": "Podman CLI", "ready": podman, "required": true },
            { "label": "Forms 14c container running", "ready": forms, "required": true },
            { "label": "Forms servlet responding on localhost:7001", "ready": listener, "required": true },
            { "label": "FSAL launcher JAR in the reference installation", "ready": jar, "required": true },
            { "label": "Reference launcher script in this checkout", "ready": tooling, "required": true },
            { "label": "Oracle Free container running (needed for the local database workflow)", "ready": database, "required": false }
        ], "note": "Checks run on the Rust server's computer. Container availability does not verify database credentials, schema contents, compiler success or visual parity." })
}
