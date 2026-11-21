//! Adapter for the backup engines: restic makes snapshots; rclone carries
//! them to SMB and SFTP destinations (and, later, cloud drives).
//!
//! Fennoly never writes restic or rclone config files: every destination is
//! described through environment variables for one command at a time, so no
//! password ever lands on disk in plain text.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::error::{AppError, AppResult};

/// Name of the rclone remote defined through environment variables.
const REMOTE: &str = "FENNOLY";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Local,
    Smb,
    Sftp,
    S3,
}

impl Kind {
    pub fn parse(s: &str) -> AppResult<Self> {
        Ok(match s {
            "local" => Kind::Local,
            "smb" => Kind::Smb,
            "sftp" => Kind::Sftp,
            "s3" => Kind::S3,
            _ => return Err(AppError::bad("unknown destination type")),
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Local => "local",
            Kind::Smb => "smb",
            Kind::Sftp => "sftp",
            Kind::S3 => "s3",
        }
    }

    pub fn needs_rclone(self) -> bool {
        matches!(self, Kind::Smb | Kind::Sftp)
    }
}

/// Settings that are safe to show in the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bucket: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

/// Settings stored encrypted and never sent back to the browser.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecretSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_key: Option<String>,
}

/// Everything needed to reach one destination.
#[derive(Debug, Clone)]
pub struct Destination {
    pub kind: Kind,
    pub settings: Settings,
    pub secrets: SecretSettings,
    /// The repository password; `None` means the backup is not encrypted.
    pub repo_password: Option<String>,
}

fn need<'a>(v: &'a Option<String>, what: &str) -> AppResult<&'a str> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::bad(format!("{what} is required")))
}

fn clean_rel(p: &str) -> String {
    p.trim().trim_matches(|c| c == '/' || c == '\\').replace('\\', "/")
}

/// Checks a destination's settings before saving them.
pub fn validate(kind: Kind, s: &Settings, sec: &SecretSettings) -> AppResult<()> {
    let no_control = |v: &Option<String>| v.as_deref().is_none_or(|x| !x.chars().any(char::is_control));
    if ![&s.path, &s.host, &s.share, &s.user, &s.domain, &s.endpoint, &s.bucket, &s.region].iter().all(|v| no_control(v)) {
        return Err(AppError::bad("settings cannot contain control characters"));
    }
    match kind {
        Kind::Local => {
            let path = need(&s.path, "folder")?;
            if !Path::new(path).is_absolute() {
                return Err(AppError::bad("use a full folder path, for example D:\\Backups or /mnt/usb/backups"));
            }
        }
        Kind::Smb => {
            need(&s.host, "server")?;
            need(&s.share, "share")?;
            need(&s.user, "user name")?;
            need(&sec.password, "password")?;
        }
        Kind::Sftp => {
            need(&s.host, "server")?;
            need(&s.user, "user name")?;
            need(&s.path, "folder")?;
            need(&sec.password, "password")?;
        }
        Kind::S3 => {
            let endpoint = need(&s.endpoint, "endpoint")?;
            if !(endpoint.starts_with("https://") || endpoint.starts_with("http://")) {
                return Err(AppError::bad("the endpoint must start with https://"));
            }
            need(&s.bucket, "bucket")?;
            need(&sec.access_key, "access key")?;
            need(&sec.secret_key, "secret key")?;
        }
    }
    Ok(())
}

/// A short human description of where backups go, shown in the UI.
pub fn describe(kind: Kind, s: &Settings) -> String {
    let o = |v: &Option<String>| v.clone().unwrap_or_default();
    match kind {
        Kind::Local => o(&s.path),
        Kind::Smb => format!("\\\\{}\\{}{}", o(&s.host), o(&s.share), s.path.as_deref().map(|p| format!("\\{}", clean_rel(p).replace('/', "\\"))).unwrap_or_default()),
        Kind::Sftp => format!("{}@{}:{}", o(&s.user), o(&s.host), o(&s.path)),
        Kind::S3 => format!("{}/{}{}", o(&s.endpoint).trim_end_matches('/'), o(&s.bucket), s.path.as_deref().map(|p| format!("/{}", clean_rel(p))).unwrap_or_default()),
    }
}

/// Where the engines are installed.
#[derive(Debug, Clone)]
pub struct Engine {
    pub restic: Option<PathBuf>,
    pub rclone: Option<PathBuf>,
    pub cache_dir: PathBuf,
}

fn find(var: &str, names: &[&str]) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| p.is_file())
}

/// The repository location and the environment that describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub url: String,
    pub env: Vec<(String, String)>,
}

impl Engine {
    pub fn detect(cache_dir: PathBuf) -> Self {
        let (restic, rclone): (&[&str], &[&str]) = if cfg!(windows) {
            (&["restic.exe", "restic"], &["rclone.exe", "rclone"])
        } else {
            (&["restic"], &["rclone"])
        };
        Engine { restic: find("FENNOLY_RESTIC", restic), rclone: find("FENNOLY_RCLONE", rclone), cache_dir }
    }

    fn restic(&self) -> AppResult<&Path> {
        self.restic.as_deref().ok_or_else(|| {
            AppError::Unavailable("the backup engine (restic) is not installed on the server".into())
        })
    }

    fn rclone(&self) -> AppResult<&Path> {
        self.rclone.as_deref().ok_or_else(|| {
            AppError::Unavailable("rclone is needed for network shares but is not installed on the server".into())
        })
    }

    /// rclone stores passwords "obscured" (reversible, not encryption); ask
    /// rclone itself to do it so the format always matches.
    async fn obscure(&self, secret: &str) -> AppResult<String> {
        let mut child = Command::new(self.rclone()?)
            .args(["obscure", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        child.stdin.take().expect("piped").write_all(secret.as_bytes()).await?;
        let out = child.wait_with_output().await?;
        if !out.status.success() {
            return Err(AppError::Internal(anyhow::anyhow!("rclone obscure failed")));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Builds the repository URL and environment for a destination.
    pub async fn repo(&self, d: &Destination) -> AppResult<Repo> {
        let s = &d.settings;
        let mut env = Vec::new();
        let rclone_var = |k: &str| format!("RCLONE_CONFIG_{REMOTE}_{k}");
        let url = match d.kind {
            Kind::Local => need(&s.path, "folder")?.to_string(),
            Kind::S3 => {
                env.push(("AWS_ACCESS_KEY_ID".into(), need(&d.secrets.access_key, "access key")?.to_string()));
                env.push(("AWS_SECRET_ACCESS_KEY".into(), need(&d.secrets.secret_key, "secret key")?.to_string()));
                if let Some(r) = s.region.as_deref().filter(|r| !r.trim().is_empty()) {
                    env.push(("AWS_DEFAULT_REGION".into(), r.trim().to_string()));
                }
                let prefix = s.path.as_deref().map(clean_rel).filter(|p| !p.is_empty()).map(|p| format!("/{p}")).unwrap_or_default();
                format!("s3:{}/{}{}", need(&s.endpoint, "endpoint")?.trim_end_matches('/'), need(&s.bucket, "bucket")?, prefix)
            }
            Kind::Smb => {
                env.push((rclone_var("TYPE"), "smb".into()));
                env.push((rclone_var("HOST"), need(&s.host, "server")?.to_string()));
                env.push((rclone_var("USER"), need(&s.user, "user name")?.to_string()));
                env.push((rclone_var("PASS"), self.obscure(need(&d.secrets.password, "password")?).await?));
                if let Some(dom) = s.domain.as_deref().filter(|v| !v.trim().is_empty()) {
                    env.push((rclone_var("DOMAIN"), dom.trim().to_string()));
                }
                if let Some(port) = s.port {
                    env.push((rclone_var("PORT"), port.to_string()));
                }
                let mut path = clean_rel(need(&s.share, "share")?);
                if let Some(sub) = s.path.as_deref().map(clean_rel).filter(|p| !p.is_empty()) {
                    path = format!("{path}/{sub}");
                }
                format!("rclone:{REMOTE}:{path}")
            }
            Kind::Sftp => {
                env.push((rclone_var("TYPE"), "sftp".into()));
                env.push((rclone_var("HOST"), need(&s.host, "server")?.to_string()));
                env.push((rclone_var("USER"), need(&s.user, "user name")?.to_string()));
                env.push((rclone_var("PASS"), self.obscure(need(&d.secrets.password, "password")?).await?));
                if let Some(port) = s.port {
                    env.push((rclone_var("PORT"), port.to_string()));
                }
                format!("rclone:{REMOTE}:{}", need(&s.path, "folder")?.trim())
            }
        };
        Ok(Repo { url, env })
    }

    async fn command(&self, d: &Destination, args: &[&str]) -> AppResult<Command> {
        let repo = self.repo(d).await?;
        let mut cmd = Command::new(self.restic()?);
        cmd.env("RESTIC_REPOSITORY", &repo.url)
            .env("RESTIC_CACHE_DIR", &self.cache_dir)
            .env("RESTIC_PROGRESS_FPS", "1")
            .envs(repo.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if d.kind.needs_rclone() {
            cmd.arg("-o").arg(format!("rclone.program={}", self.rclone()?.display()));
        }
        match &d.repo_password {
            Some(pw) => {
                cmd.env("RESTIC_PASSWORD", pw);
            }
            None => {
                cmd.arg("--insecure-no-password");
            }
        }
        cmd.args(args);
        Ok(cmd)
    }

    /// Runs restic and returns stdout. Each stdout line is passed to `on_line`
    /// as it arrives (restic `--json` prints one message per line).
    pub async fn run(&self, d: &Destination, args: &[&str], mut on_line: impl FnMut(&str)) -> AppResult<String> {
        let mut child = self.command(d, args).await?.spawn()?;
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let err_task = tokio::spawn(async move {
            let mut buf = String::new();
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if buf.len() < 16_000 {
                    buf.push_str(&line);
                    buf.push('\n');
                }
            }
            buf
        });
        let mut out = String::new();
        let mut lines = BufReader::new(stdout).lines();
        while let Some(line) = lines.next_line().await? {
            on_line(&line);
            if out.len() < 4_000_000 {
                out.push_str(&line);
                out.push('\n');
            }
        }
        let status = child.wait().await?;
        let stderr = err_task.await.unwrap_or_default();
        if !status.success() {
            return Err(AppError::BadRequest(explain(&stderr, &out)));
        }
        Ok(out)
    }

    /// Creates the repository if it does not exist yet.
    pub async fn init(&self, d: &Destination) -> AppResult<()> {
        if self.run(d, &["cat", "config"], |_| {}).await.is_ok() {
            return Ok(());
        }
        match self.run(d, &["init"], |_| {}).await {
            Ok(_) => Ok(()),
            Err(AppError::BadRequest(m)) if m.contains("already") => Ok(()),
            Err(e) => Err(e),
        }
    }
}

/// Turns engine error output into a sentence a family admin can act on.
pub fn explain(stderr: &str, stdout: &str) -> String {
    let all = format!("{stderr}\n{stdout}").to_lowercase();
    let pick = |patterns: &[&str]| patterns.iter().any(|p| all.contains(p));
    if pick(&["wrong password", "no key found"]) {
        return "The recovery key does not match this backup destination.".into();
    }
    if pick(&["no space left", "disk full", "insufficient storage", "quota"]) {
        return "The backup destination is full.".into();
    }
    if pick(&["permission denied", "access is denied", "access denied", "logon failure", "authentication failed", "status_logon_failure", "invalidaccesskeyid", "signaturedoesnotmatch", "403"]) {
        return "The destination refused access. Check the user name, password or keys.".into();
    }
    if pick(&["no such host", "connection refused", "i/o timeout", "network is unreachable", "timed out", "dial tcp", "unreachable"]) {
        return "Cannot reach the backup destination. Check that it is switched on and connected.".into();
    }
    // Before the generic "missing path" check: a missing repository also
    // reports "no such file or directory" for its config file.
    if pick(&["unable to open config file", "repository does not exist", "is there a repository at the following location"]) {
        return "No backup was found at this destination. If it is a USB drive or network share, check that it is connected.".into();
    }
    if pick(&["the system cannot find the path", "no such file or directory", "cannot find the file", "is not a directory"]) {
        return "The backup folder does not exist. Check the path, or connect the drive.".into();
    }
    if pick(&["unable to create lock", "repository is already locked"]) {
        return "Another backup is using this destination right now. Try again later.".into();
    }
    let tail: String = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("unknown error").chars().take(300).collect();
    format!("The backup engine reported: {tail}")
}

/// restic's final `--json` summary of a backup.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Summary {
    #[serde(default)]
    pub snapshot_id: Option<String>,
    #[serde(default)]
    pub files_new: i64,
    #[serde(default)]
    pub files_changed: i64,
    #[serde(default)]
    pub data_added: i64,
    #[serde(default)]
    pub total_files_processed: i64,
    #[serde(default)]
    pub total_bytes_processed: i64,
}

/// A progress update from `restic backup --json`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Progress {
    #[serde(default)]
    pub percent_done: f64,
    #[serde(default)]
    pub bytes_done: i64,
    #[serde(default)]
    pub total_bytes: i64,
}

pub enum BackupMessage {
    Progress(Progress),
    Summary(Summary),
    Other,
}

pub fn parse_backup_line(line: &str) -> BackupMessage {
    #[derive(Deserialize)]
    struct Tag {
        message_type: Option<String>,
    }
    let Ok(tag) = serde_json::from_str::<Tag>(line) else { return BackupMessage::Other };
    match tag.message_type.as_deref() {
        Some("status") => serde_json::from_str(line).map(BackupMessage::Progress).unwrap_or(BackupMessage::Other),
        Some("summary") => serde_json::from_str(line).map(BackupMessage::Summary).unwrap_or(BackupMessage::Other),
        _ => BackupMessage::Other,
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Snapshot {
    pub id: String,
    #[serde(default)]
    pub short_id: String,
    pub time: String,
    #[serde(default)]
    pub paths: Vec<String>,
}

/// One entry from `restic ls --json`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LsEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default)]
    pub mtime: Option<String>,
}

/// Parses `restic ls --json` output (snapshot header line, then one node per line).
pub fn parse_ls(out: &str) -> Vec<LsEntry> {
    #[derive(Deserialize)]
    struct Tag {
        struct_type: Option<String>,
        message_type: Option<String>,
    }
    out.lines()
        .filter(|l| {
            serde_json::from_str::<Tag>(l)
                .is_ok_and(|t| t.struct_type.as_deref() == Some("node") || t.message_type.as_deref() == Some("node"))
        })
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// How restic writes a local path inside a snapshot: `/data/spaces` on Unix,
/// `/E/projects/fennoly/data/spaces` for `E:\projects\fennoly\data\spaces` on Windows.
pub fn snapshot_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.strip_prefix("//?/").unwrap_or(&s).to_string();
    match s.as_bytes() {
        [drive, b':', rest @ ..] if drive.is_ascii_alphabetic() => {
            format!("/{}{}", *drive as char, String::from_utf8_lossy(rest))
        }
        _ => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dest(kind: Kind, settings: Settings, secrets: SecretSettings) -> Destination {
        Destination { kind, settings, secrets, repo_password: Some("pw".into()) }
    }

    #[test]
    fn validation() {
        let abs = if cfg!(windows) { "D:\\Backups" } else { "/mnt/usb" };
        assert!(validate(Kind::Local, &Settings { path: Some(abs.into()), ..Default::default() }, &Default::default()).is_ok());
        assert!(validate(Kind::Local, &Settings { path: Some("backups".into()), ..Default::default() }, &Default::default()).is_err());
        let smb = Settings { host: Some("nas".into()), share: Some("backup".into()), user: Some("noor".into()), ..Default::default() };
        assert!(validate(Kind::Smb, &smb, &Default::default()).is_err(), "password required");
        assert!(validate(Kind::Smb, &smb, &SecretSettings { password: Some("x".into()), ..Default::default() }).is_ok());
        let s3 = Settings { endpoint: Some("s3.example.com".into()), bucket: Some("b".into()), ..Default::default() };
        let keys = SecretSettings { access_key: Some("a".into()), secret_key: Some("s".into()), ..Default::default() };
        assert!(validate(Kind::S3, &s3, &keys).is_err(), "endpoint needs a scheme");
        assert!(validate(Kind::S3, &Settings { endpoint: Some("https://s3.eu-central-003.backblazeb2.com".into()), ..s3 }, &keys).is_ok());
        let bad = Settings { path: Some("/x\n--y".into()), ..Default::default() };
        assert!(validate(Kind::Local, &bad, &Default::default()).is_err());
    }

    #[tokio::test]
    async fn s3_and_local_repositories() {
        let engine = Engine { restic: None, rclone: None, cache_dir: "c".into() };
        let s3 = dest(
            Kind::S3,
            Settings { endpoint: Some("https://s3.example.com/".into()), bucket: Some("family".into()), path: Some("/fennoly/".into()), region: Some("eu".into()), ..Default::default() },
            SecretSettings { access_key: Some("AK".into()), secret_key: Some("SK".into()), ..Default::default() },
        );
        let repo = engine.repo(&s3).await.unwrap();
        assert_eq!(repo.url, "s3:https://s3.example.com/family/fennoly");
        assert!(repo.env.contains(&("AWS_ACCESS_KEY_ID".into(), "AK".into())));
        assert!(repo.env.contains(&("AWS_DEFAULT_REGION".into(), "eu".into())));
        let local = dest(Kind::Local, Settings { path: Some("/mnt/usb/fennoly".into()), ..Default::default() }, Default::default());
        assert_eq!(engine.repo(&local).await.unwrap().url, "/mnt/usb/fennoly");
    }

    #[tokio::test]
    async fn missing_engines_are_reported_plainly() {
        let engine = Engine { restic: None, rclone: None, cache_dir: "c".into() };
        let smb = dest(
            Kind::Smb,
            Settings { host: Some("nas".into()), share: Some("b".into()), user: Some("u".into()), ..Default::default() },
            SecretSettings { password: Some("p".into()), ..Default::default() },
        );
        assert!(matches!(engine.repo(&smb).await, Err(AppError::Unavailable(_))));
        let local = dest(Kind::Local, Settings { path: Some("/x".into()), ..Default::default() }, Default::default());
        assert!(matches!(engine.run(&local, &["snapshots"], |_| {}).await, Err(AppError::Unavailable(_))));
    }

    #[test]
    fn descriptions() {
        let smb = Settings { host: Some("nas".into()), share: Some("backup".into()), path: Some("fennoly".into()), ..Default::default() };
        assert_eq!(describe(Kind::Smb, &smb), "\\\\nas\\backup\\fennoly");
        let sftp = Settings { host: Some("pi".into()), user: Some("u".into()), path: Some("/srv/b".into()), ..Default::default() };
        assert_eq!(describe(Kind::Sftp, &sftp), "u@pi:/srv/b");
    }

    #[test]
    fn error_messages_are_plain() {
        assert!(explain("Fatal: wrong password or no key found", "").contains("recovery key"));
        assert!(explain("dial tcp 10.0.0.5:445: i/o timeout", "").contains("Cannot reach"));
        assert!(explain("Fatal: unable to open config file: Stat: stat /x/config: no such file or directory
Is there a repository at the following location?", "").starts_with("No backup was found"));
        assert!(explain("write /mnt/usb/data/ab: no space left on device", "").contains("full"));
        assert!(explain("something odd happened", "").contains("something odd happened"));
    }

    #[test]
    fn parses_restic_json() {
        let status = r#"{"message_type":"status","percent_done":0.5,"total_files":10,"files_done":5,"total_bytes":1000,"bytes_done":500}"#;
        assert!(matches!(parse_backup_line(status), BackupMessage::Progress(p) if p.percent_done == 0.5 && p.bytes_done == 500));
        let summary = r#"{"message_type":"summary","files_new":3,"files_changed":1,"files_unmodified":6,"dirs_new":1,"data_added":12345,"total_files_processed":10,"total_bytes_processed":99999,"total_duration":1.5,"snapshot_id":"abc123"}"#;
        match parse_backup_line(summary) {
            BackupMessage::Summary(s) => {
                assert_eq!(s.snapshot_id.as_deref(), Some("abc123"));
                assert_eq!((s.files_new, s.data_added, s.total_files_processed), (3, 12345, 10));
            }
            _ => panic!("summary not parsed"),
        }
        assert!(matches!(parse_backup_line("not json"), BackupMessage::Other));
        let ls = concat!(
            r#"{"time":"2026-11-22T02:00:00Z","paths":["/data/spaces"],"id":"x","short_id":"x","struct_type":"snapshot"}"#, "\n",
            r#"{"name":"spaces","type":"dir","path":"/data/spaces","struct_type":"node"}"#, "\n",
            r#"{"name":"a.pdf","type":"file","path":"/data/spaces/users/noor/a.pdf","size":42,"mtime":"2026-11-21T10:00:00Z","message_type":"node"}"#
        );
        let nodes = parse_ls(ls);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[1].size, Some(42));
    }

    #[test]
    fn windows_paths_map_like_restic() {
        assert_eq!(snapshot_path(Path::new("/data/spaces")), "/data/spaces");
        assert_eq!(snapshot_path(Path::new(r"E:\projects\fennoly\data\spaces")), "/E/projects/fennoly/data/spaces");
        assert_eq!(snapshot_path(Path::new(r"\\?\E:\data")), "/E/data");
    }
}
