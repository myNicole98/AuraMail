use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const RELEASES_LATEST: &str =
    "https://git.nikkosphere.com/api/v1/repos/nicole/AuraMail/releases/latest";
const RELEASE_PAGE: &str = "https://git.nikkosphere.com/nicole/AuraMail/releases";
const DOWNLOAD_HOST: &str = "git.nikkosphere.com";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub page_url: String,
    pub download_url: Option<String>,
    pub checksums_url: Option<String>,
    pub asset_name: Option<String>,
    pub checksum: Option<String>,
    pub can_install: bool,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Ask the release server whether a newer stable release exists. Any network
/// or parsing failure counts as "no update": the notice is a convenience, not
/// a reason to surface an error.
pub fn check(current: &str) -> Option<UpdateInfo> {
    // Test hook: pretend a release exists without touching the server.
    if let Ok(fake) = std::env::var("AURAMAIL_FAKE_UPDATE") {
        let fake = fake.trim().trim_start_matches(['v', 'V']);
        if !fake.is_empty() {
            return Some(UpdateInfo {
                version: fake.to_string(),
                page_url: RELEASE_PAGE.into(),
                download_url: None,
                checksums_url: None,
                asset_name: None,
                checksum: None,
                can_install: false,
            });
        }
    }

    let release = latest_release()?;
    if release.draft || release.prerelease || !is_newer(current, &release.tag_name) {
        return None;
    }

    let asset = pick_asset(&release.assets)?;
    let checksums = release
        .assets
        .iter()
        .find(|candidate| candidate.name == "SHA256SUMS")
        .and_then(|candidate| fetch_checksums(&candidate.browser_download_url));
    let checksum = checksums.and_then(|lines| {
        lines
            .into_iter()
            .find(|(name, _)| name == &asset.name)
            .map(|(_, hash)| hash)
    });
    let can_install = checksum.is_some()
        && (cfg!(target_os = "linux") || cfg!(target_os = "macos") || cfg!(target_os = "windows"))
        && (cfg!(not(target_os = "linux")) || appimage_path().is_some());

    Some(UpdateInfo {
        version: release.tag_name.trim_start_matches(['v', 'V']).to_string(),
        page_url: release.html_url,
        download_url: Some(asset.browser_download_url),
        checksums_url: release
            .assets
            .iter()
            .find(|candidate| candidate.name == "SHA256SUMS")
            .map(|candidate| candidate.browser_download_url.clone()),
        asset_name: Some(asset.name),
        checksum,
        can_install,
    })
}

fn latest_release() -> Option<Release> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("AuraMail/update-check")
        .build()
        .ok()?;
    client
        .get(RELEASES_LATEST)
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()
}

fn pick_asset(assets: &[Asset]) -> Option<Asset> {
    let suffix = platform_suffix();
    assets
        .iter()
        .find(|asset| asset.name.ends_with(suffix))
        .cloned()
}

fn platform_suffix() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "-x86_64.AppImage"
    }
    #[cfg(target_os = "macos")]
    {
        "-macos-update.zip"
    }
    #[cfg(target_os = "windows")]
    {
        "-windows-update.zip"
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        ".unsupported"
    }
}

fn fetch_checksums(url: &str) -> Option<Vec<(String, String)>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .ok()?;
    let body = client.get(url).send().ok()?.error_for_status().ok()?.text().ok()?;
    Some(parse_checksums(&body))
}

fn parse_checksums(body: &str) -> Vec<(String, String)> {
    body.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?.to_ascii_lowercase();
            let name = parts.next()?.trim_start_matches('*').to_string();
            (hash.len() == 64 && !name.is_empty()).then_some((name, hash))
        })
        .collect()
}

/// Path of the AppImage currently running, if there is one. Nix's
/// `appimage-run` extracts the image and does not export `APPIMAGE`, so fall
/// back to the image named in the launcher's command line.
pub fn appimage_path() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("APPIMAGE") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
    }
    let mut pid = std::process::id();
    for _ in 0..6 {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let ppid: u32 = stat
            .rsplit(')')
            .next()?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()?;
        if ppid <= 1 {
            break;
        }
        if let Ok(cmdline) = std::fs::read(format!("/proc/{ppid}/cmdline")) {
            for arg in cmdline.split(|byte| *byte == 0) {
                if arg.is_empty() {
                    continue;
                }
                let candidate = String::from_utf8_lossy(arg);
                if candidate.to_ascii_lowercase().ends_with(".appimage") {
                    let path = PathBuf::from(candidate.as_ref());
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
        }
        pid = ppid;
    }
    None
}

pub fn verify_url(url: &str) -> bool {
    reqwest::Url::parse(url)
        .map(|parsed| parsed.scheme() == "https" && parsed.host_str() == Some(DOWNLOAD_HOST))
        .unwrap_or(false)
}

pub fn safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && name != "."
        && name != ".."
}

pub fn download(
    url: &str,
    destination: &Path,
    expected: &str,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(1800))
        .user_agent("AuraMail/update")
        .build()
        .map_err(|error| error.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let total = response.content_length();
    let mut file = File::create(destination).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = response.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| error.to_string())?;
        hasher.update(&buffer[..read]);
        downloaded += read as u64;
        on_progress(downloaded, total);
    }
    file.flush().map_err(|error| error.to_string())?;
    let digest = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if digest != expected.to_ascii_lowercase() {
        let _ = std::fs::remove_file(destination);
        return Err("The downloaded update did not match its checksum.".into());
    }
    Ok(())
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let trimmed = value.trim().trim_start_matches(['v', 'V']);
    let core = trimmed.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// True only for a strictly newer stable release. Pre-release tags are never
/// advertised automatically.
pub fn is_newer(current: &str, candidate: &str) -> bool {
    let candidate = candidate.trim().trim_start_matches(['v', 'V']);
    if candidate.contains('-') {
        return false;
    }
    match (parse_version(current), parse_version(candidate)) {
        (Some(current), Some(candidate)) => candidate > current,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_newer, parse_checksums, safe_file_name, verify_url};

    #[test]
    fn compares_each_version_component_numerically() {
        assert!(is_newer("0.6.1", "0.6.2"));
        assert!(is_newer("0.6.9", "0.6.10"));
        assert!(is_newer("0.9.9", "0.10.0"));
        assert!(is_newer("1.0.0", "2.0.0"));
        assert!(!is_newer("0.6.1", "0.6.1"));
        assert!(!is_newer("0.6.2", "0.6.1"));
    }

    #[test]
    fn accepts_a_v_prefix_and_ignores_noise() {
        assert!(is_newer("0.6.1", "v0.6.2"));
        assert!(is_newer("v0.6.1", "0.6.2"));
        assert!(!is_newer("0.6.1", "nightly"));
        assert!(!is_newer("0.6.1", "0.6.2-beta.1"));
    }

    #[test]
    fn omitted_components_count_as_zero() {
        assert!(is_newer("0.6", "0.6.1"));
        assert!(!is_newer("0.6.1", "0.6"));
    }

    #[test]
    fn only_our_https_release_host_is_accepted() {
        assert!(verify_url(
            "https://git.nikkosphere.com/nicole/AuraMail/releases/download/v0.6.4/a.zip"
        ));
        assert!(!verify_url("http://git.nikkosphere.com/a.zip"));
        assert!(!verify_url("https://example.com/a.zip"));
    }

    #[test]
    fn parses_sha256sum_files() {
        let sums = parse_checksums(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  AuraMail-0.6.4-x86_64.AppImage\n\
             bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb *AuraMail-0.6.4-macos-update.zip\n\
             not-a-checksum  SKIP\n",
        );
        assert_eq!(sums.len(), 2);
        assert_eq!(sums[0].0, "AuraMail-0.6.4-x86_64.AppImage");
        assert_eq!(sums[1].0, "AuraMail-0.6.4-macos-update.zip");
    }

    #[test]
    fn rejects_paths_in_asset_names() {
        assert!(safe_file_name("AuraMail-0.6.4-x86_64.AppImage"));
        assert!(!safe_file_name("../auramail"));
        assert!(!safe_file_name("a/b"));
        assert!(!safe_file_name(".."));
    }
}

#[cfg(target_os = "linux")]
const LINUX_HELPER: &str = r#"#!/bin/sh
pid="$1"; staged="$2"; target="$3"
while kill -0 "$pid" 2>/dev/null; do sleep 0.2; done
next="$target.auramail-new"
cp -f "$staged" "$next" || exit 1
chmod 755 "$next" || exit 1
mv -f "$next" "$target" || exit 1
"$target" >/dev/null 2>&1 &
"#;

#[cfg(target_os = "macos")]
const MAC_HELPER: &str = r#"#!/bin/sh
pid="$1"; zip="$2"; target="$3"
while kill -0 "$pid" 2>/dev/null; do sleep 0.2; done
tmp="$(mktemp -d)"
ditto -x -k "$zip" "$tmp" || exit 1
old="$target.old"
rm -rf "$old"
mv "$target" "$old" || exit 1
if ! mv "$tmp/AuraMail.app" "$target"; then
  mv "$old" "$target"
  exit 1
fi
rm -rf "$old" "$tmp"
open "$target"
"#;

#[cfg(target_os = "windows")]
const WINDOWS_HELPER: &str = r#"param([int]$ProcessId, [string]$Zip, [string]$Exe)
$ErrorActionPreference = "Stop"
$log = Join-Path $env:TEMP "auramail-update.log"
function Write-Log([string]$message) {
  Add-Content -Path $log -Value ("{0} {1}" -f (Get-Date -Format o), $message)
}
function Replace-ItemWithRetry([string]$source, [string]$target) {
  for ($i = 0; $i -lt 30; $i++) {
    try {
      if (Test-Path $target) { Remove-Item $target -Force -Recurse }
      Copy-Item $source $target -Force -Recurse
      return
    } catch {
      Start-Sleep -Milliseconds 500
    }
  }
  throw "could not replace $target"
}
$stage = $null
try {
  # Prove to the app that PowerShell actually executed this file before it
  # exits. Without this the app can close into the void when the hand-off
  # fails, with nothing but a spinning cursor to show for it.
  Set-Content -Path (Join-Path $env:TEMP "auramail-update.started") -Value $PID
  Write-Log ("helper started: waiting for process $ProcessId, zip=$Zip, exe=$Exe")
  $deadline = (Get-Date).AddSeconds(90)
  while ((Get-Process -Id $ProcessId -ErrorAction SilentlyContinue) -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 200
  }
  # Give Windows a moment to release the image handle.
  Start-Sleep -Milliseconds 800
  if (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue) {
    throw "the app did not exit before the update timed out"
  }

  $dir = Split-Path -Parent $Exe
  $stage = Join-Path $env:TEMP ("auramail-update-" + [guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Path $stage | Out-Null
  Write-Log "extracting $Zip to $stage"
  Expand-Archive -Path $Zip -DestinationPath $stage -Force
  if (!(Test-Path (Join-Path $stage "AuraMail.exe"))) {
    throw "the update archive did not contain AuraMail.exe"
  }

  Write-Log "replacing files in $dir"
  foreach ($name in @("AuraMail.exe", "auramail-proton-bridge.exe")) {
    $source = Join-Path $stage $name
    if (Test-Path $source) {
      Replace-ItemWithRetry $source (Join-Path $dir $name)
    }
  }
  $sourceResources = Join-Path $stage "resources"
  if (Test-Path $sourceResources) {
    Replace-ItemWithRetry $sourceResources (Join-Path $dir "resources")
  }

  if (Test-Path $Exe) {
    Write-Log ("installed exe version: " + (Get-Item $Exe).VersionInfo.FileVersion)
  }
  Write-Log "starting $Exe"
  Start-Process -FilePath $Exe -WorkingDirectory $dir
  Write-Log "started"
} catch {
  Write-Log ("FAILED: " + $_.Exception.Message)
  throw
} finally {
  if ($stage -and (Test-Path $stage)) {
    Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
  }
}
"#;

/// Spawn a detached helper that waits for this process to exit, replaces the
/// installed app with the staged update and relaunches it.
pub fn install(staged: &Path) -> crate::error::AppResult<()> {
    use crate::error::AppError;
    #[cfg(unix)]
    use std::process::{Command, Stdio};

    #[cfg(not(target_os = "linux"))]
    let exe = std::env::current_exe()?;
    let script_dir = std::env::temp_dir().join("auramail-update");
    std::fs::create_dir_all(&script_dir)?;

    #[cfg(target_os = "linux")]
    {
        let target = appimage_path().ok_or_else(|| {
            AppError::Message(
                "Automatic updates are not available for this installation.".into(),
            )
        })?;
        let script = script_dir.join("auramail-update.sh");
        std::fs::write(&script, LINUX_HELPER)?;
        let mut command = Command::new("/bin/sh");
        command
            .arg(&script)
            .arg(std::process::id().to_string())
            .arg(staged)
            .arg(&target);
        detach(&mut command, Stdio::null(), Stdio::null())?;
    }

    #[cfg(target_os = "macos")]
    {
        let bundle = exe
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or_else(|| AppError::Message("Could not locate the application bundle.".into()))?;
        let script = script_dir.join("auramail-update.sh");
        std::fs::write(&script, MAC_HELPER)?;
        let mut command = Command::new("/bin/sh");
        command
            .arg(&script)
            .arg(std::process::id().to_string())
            .arg(staged)
            .arg(bundle);
        detach(&mut command, Stdio::null(), Stdio::null())?;
    }

    #[cfg(target_os = "windows")]
    {
        let script = script_dir.join("auramail-update.ps1");
        std::fs::write(&script, WINDOWS_HELPER)?;
        let _ = std::fs::remove_file(windows_log());
        let _ = std::fs::remove_file(windows_marker());
        append_windows_log(&format!(
            "install: pid={} script={} zip={} exe={}",
            std::process::id(),
            script.display(),
            staged.display(),
            exe.display()
        ));
        if let Err(error) = spawn_windows_helper(&script, staged, &exe) {
            append_windows_log(&format!("install: could not start powershell: {error}"));
            return Err(AppError::Message(format!(
                "AuraMail could not start the update helper ({error}), so nothing was changed. See %TEMP%\\auramail-update.log."
            )));
        }
        if !wait_for_windows_helper(Duration::from_secs(5)) {
            append_windows_log(
                "install: powershell started but never ran the helper; see auramail-update.stderr.log",
            );
            return Err(AppError::Message(
                "AuraMail could not hand the update to its helper, so nothing was changed. See %TEMP%\\auramail-update.log.".into(),
            ));
        }
        append_windows_log("install: helper is running, exiting");
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_log() -> PathBuf {
    std::env::temp_dir().join("auramail-update.log")
}

#[cfg(target_os = "windows")]
fn windows_marker() -> PathBuf {
    std::env::temp_dir().join("auramail-update.started")
}

#[cfg(target_os = "windows")]
fn append_windows_log(message: &str) {
    use std::io::Write as _;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(windows_log())
    {
        let _ = writeln!(file, "{message}");
    }
}

/// Start the hidden PowerShell helper. `CREATE_NO_WINDOW` gives it a console
/// with no window, which is what a console host such as PowerShell expects;
/// `DETACHED_PROCESS` would leave it with no console at all, and combining the
/// two silently drops `CREATE_NO_WINDOW`.
#[cfg(target_os = "windows")]
fn spawn_windows_helper(script: &Path, staged: &Path, exe: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let stdout = std::fs::File::create(std::env::temp_dir().join("auramail-update.stdout.log"))?;
    let stderr = std::fs::File::create(std::env::temp_dir().join("auramail-update.stderr.log"))?;
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
        ])
        .arg(script)
        .arg("-ProcessId")
        .arg(std::process::id().to_string())
        .arg("-Zip")
        .arg(staged)
        .arg("-Exe")
        .arg(exe)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    command.spawn()?;
    Ok(())
}

/// Wait for the helper to confirm it ran by writing its start marker. The app
/// must not exit until this succeeds, otherwise a failed hand-off looks
/// exactly like a successful one.
#[cfg(target_os = "windows")]
fn wait_for_windows_helper(timeout: Duration) -> bool {
    let marker = windows_marker();
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if marker.is_file() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    marker.is_file()
}

#[cfg(unix)]
fn detach(
    command: &mut std::process::Command,
    stdout: std::process::Stdio,
    stderr: std::process::Stdio,
) -> std::io::Result<()> {
    command.stdin(std::process::Stdio::null()).stdout(stdout).stderr(stderr);
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    command.spawn()?;
    Ok(())
}
