//! Launch the game with a temporary config, mods snapshot, and export scenario.

use std::{
    env,
    ffi::OsStr,
    fs::{self, File},
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use tempfile::{NamedTempFile, TempDir};
use wait_timeout::ChildExt;

use crate::GameData;

#[derive(Debug)]
pub struct ExtractOptions {
    /// Full path to the Factorio executable (2.1.24+).
    pub factorio: PathBuf,
    /// Mods and mod-settings.dat to snapshot; None uses installed built-in mods.
    pub mod_directory: Option<PathBuf>,
    /// Override the data directory for a nonstandard installation.
    pub data_directory: Option<PathBuf>,
    /// Limit for each game process, including startup and prototype loading.
    pub timeout: Duration,
}

impl ExtractOptions {
    pub fn new(factorio: impl Into<PathBuf>) -> Self {
        Self {
            factorio: factorio.into(),
            mod_directory: None,
            data_directory: None,
            timeout: Duration::from_secs(300),
        }
    }
}

/// Extract all raw prototypes and a resolved catalog into `output_directory`.
///
/// Returns the catalog and writes `catalog.json`, `data-raw-dump.json`, and two
/// game logs. Each output file is replaced atomically after both game runs and
/// catalog validation succeed. The original profile, mods, and saves are unused
/// for writes. User mod files are copied, including startup settings.
pub fn extract_game_data(options: &ExtractOptions, output_directory: &Path) -> Result<GameData> {
    ensure!(
        !options.timeout.is_zero(),
        "Timeout must be greater than zero"
    );
    let executable = options.factorio.canonicalize().with_context(|| {
        format!(
            "Factorio executable not found: {}",
            options.factorio.display()
        )
    })?;
    ensure!(executable.is_file(), "Factorio executable must be a file");
    // Strip Windows' extended-path prefix before passing paths to Factorio.
    let executable = game_path(&executable);
    let installation = executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent);
    let data_directory = options
        .data_directory
        .clone()
        .or_else(|| installation.map(|p| p.join("data")))
        .context("Cannot find Factorio data directory; specify --data-dir")?;
    let data_directory = game_path(&data_directory.canonicalize().with_context(|| {
        format!(
            "Cannot open Factorio data directory: {}",
            data_directory.display()
        )
    })?);
    ensure!(
        data_directory.join("base/info.json").is_file(),
        "Missing base/info.json in Factorio data directory"
    );

    let profile = tempfile::Builder::new().prefix("factorium-").tempdir()?;
    let profile_path = game_path(profile.path());
    let mods_path = profile_path.join("mods");
    fs::create_dir(&mods_path)?;
    if let Some(source) = &options.mod_directory {
        ensure!(
            source.is_dir(),
            "Mod directory does not exist: {}",
            source.display()
        );
        copy_directory(source, &mods_path)?;
    }
    let scenario = profile_path.join("scenarios/factorium-export");
    fs::create_dir_all(&scenario)?;
    fs::write(scenario.join("control.lua"), include_str!("export.lua"))?;
    let config = profile_path.join("config.ini");
    fs::write(
        &config,
        format!(
            "[path]\nread-data={}\nwrite-data={}\n[other]\ncheck-updates=false\nenable-crash-log-uploading=false\n",
            config_path(&data_directory)?,
            config_path(&profile_path)?
        ),
    )?;
    let run = |args: &[&str], name: &str| {
        run_factorio(
            &executable,
            &config,
            &mods_path,
            &profile,
            args,
            name,
            options.timeout,
        )
    };
    let version_log = run(&["--version"], "version")?;
    check_version(&fs::read_to_string(version_log)?)?;
    run(&["--dump-data"], "dump")?;
    run(
        &["--test-scenario", "factorium-export", "--test-ticks", "2"],
        "runtime",
    )?;

    let catalog_path = profile_path.join("script-output/factorium/catalog.json");
    let catalog = GameData::from_reader(File::open(&catalog_path).context(
        "Factorio did not produce a catalog; check whether this build supports --test-scenario",
    )?)?;
    let raw_path = profile_path.join("script-output/data-raw-dump.json");
    ensure!(
        fs::metadata(&raw_path)
            .context("Factorio did not produce data-raw-dump.json")?
            .len()
            > 0,
        "Factorio produced an empty raw dump"
    );
    fs::create_dir_all(output_directory).with_context(|| {
        format!(
            "Cannot create export directory: {}",
            output_directory.display()
        )
    })?;
    // Keep the exact engine dump, without decoding hundreds of MB of sprite data.
    atomic_write(output_directory, "data-raw-dump.json", |file| {
        std::io::copy(&mut File::open(&raw_path)?, file)?;
        Ok(())
    })?;
    for name in ["dump", "runtime"] {
        atomic_write(output_directory, &format!("{name}.log"), |file| {
            std::io::copy(
                &mut File::open(profile_path.join(format!("{name}.log")))?,
                file,
            )?;
            Ok(())
        })?;
    }
    atomic_write(output_directory, "catalog.json", |file| {
        serde_json::to_writer_pretty(&mut *file, &catalog)?;
        file.write_all(b"\n")?;
        Ok(())
    })?;
    Ok(catalog)
}

fn run_factorio(
    executable: &Path,
    config: &Path,
    mods: &Path,
    profile: &TempDir,
    args: &[&str],
    name: &str,
    timeout: Duration,
) -> Result<PathBuf> {
    let log_path = profile.path().join(format!("{name}.log"));
    let log = File::create(&log_path)?;
    let mut command = Command::new(executable);
    command.args([OsStr::new("--config"), config.as_os_str()]);
    command.args([OsStr::new("--mod-directory"), mods.as_os_str()]);
    command.args(args).stdin(Stdio::null());
    command.stdout(log.try_clone()?).stderr(log);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("Cannot start {}", executable.display()))?;
    let status = match child.wait_timeout(timeout) {
        Ok(Some(status)) => status,
        result => {
            let _ = child.kill();
            let _ = child.wait();
            let details = log_tail(&log_path);
            match result {
                Ok(None) => bail!("Factorio {name} exceeded timeout of {timeout:?}.\n{details}"),
                Err(error) => {
                    return Err(error)
                        .context(format!("Cannot wait for Factorio {name}.\n{details}"));
                }
                _ => unreachable!(),
            }
        }
    };
    ensure!(
        status.success(),
        "Factorio {name} failed ({status}).\n{}",
        log_tail(&log_path)
    );
    Ok(log_path)
}

fn log_tail(path: &Path) -> String {
    use std::io::{Seek, SeekFrom};
    let Ok(mut file) = File::open(path) else {
        return "Log unavailable".into();
    };
    let _ = file
        .seek(SeekFrom::End(-16_384))
        .or_else(|_| file.seek(SeekFrom::Start(0)));
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

fn check_version(log: &str) -> Result<()> {
    let version = log
        .lines()
        .find_map(|line| line.strip_prefix("Version: "))
        .and_then(|value| value.split_whitespace().next())
        .context("Cannot read Factorio version from --version output")?;
    let parts = version
        .split('.')
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        parts.len() == 3 && (parts[0], parts[1], parts[2]) >= (2, 1, 24),
        "Factorio {version} is unsupported; this extractor requires 2.1.24+ (--test-scenario and runtime API)"
    );
    Ok(())
}

fn atomic_write(
    directory: &Path,
    name: &str,
    write: impl FnOnce(&mut dyn Write) -> Result<()>,
) -> Result<()> {
    let mut temporary = NamedTempFile::new_in(directory)?;
    {
        let mut writer = BufWriter::new(temporary.as_file_mut());
        write(&mut writer)?;
        writer.flush()?;
    }
    temporary.as_file_mut().sync_all()?;
    temporary
        .persist(directory.join(name))
        .with_context(|| format!("Cannot save {name}"))?;
    Ok(())
}

fn config_path(path: &Path) -> Result<String> {
    let text = path
        .to_str()
        .context("Factorio paths must be valid UTF-8")?;
    ensure!(
        !text.contains(['\n', '\r']),
        "Factorio path contains a newline"
    );
    Ok(text.replace('\\', "/"))
}

fn game_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    if let Some(text) = path.to_str() {
        if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
            return PathBuf::from(format!("\\\\{unc}"));
        }
        if let Some(normal) = text.strip_prefix("\\\\?\\") {
            return PathBuf::from(normal);
        }
    }
    path.to_path_buf()
}

fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    for entry in
        fs::read_dir(source).with_context(|| format!("Cannot read {}", source.display()))?
    {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        let kind = entry.file_type()?;
        ensure!(
            !kind.is_symlink(),
            "Symlinks in mods are unsupported; provide a directory with regular files: {}",
            entry.path().display()
        );
        if kind.is_dir() {
            fs::create_dir(&target)?;
            copy_directory(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            bail!("Unsupported mod file: {}", entry.path().display());
        }
    }
    Ok(())
}

/// Run a finite verification scenario using the same isolated-profile policy as
/// extraction. The Lua scenario must write `factorium/result.json` before exit.
pub(crate) fn run_test_scenario(
    options: &ExtractOptions,
    control: &str,
    ticks: u32,
) -> Result<serde_json::Value> {
    ensure!(
        !options.timeout.is_zero(),
        "Timeout must be greater than zero"
    );
    let executable = game_path(
        &options
            .factorio
            .canonicalize()
            .context("Factorio executable not found")?,
    );
    let data = options
        .data_directory
        .clone()
        .or_else(|| {
            executable
                .parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .map(|p| p.join("data"))
        })
        .context("Specify the Factorio data directory")?;
    let data = game_path(&data.canonicalize()?);
    let profile = tempfile::Builder::new()
        .prefix("factorium-verify-")
        .tempdir()?;
    let root = game_path(profile.path());
    let mods = root.join("mods");
    fs::create_dir(&mods)?;
    if let Some(source) = &options.mod_directory {
        copy_directory(source, &mods)?;
    }
    let scenario = root.join("scenarios/factorium-verify");
    fs::create_dir_all(&scenario)?;
    fs::write(scenario.join("control.lua"), control)?;
    let config = root.join("config.ini");
    fs::write(
        &config,
        format!(
            "[path]\nread-data={}\nwrite-data={}\n[other]\ncheck-updates=false\nenable-crash-log-uploading=false\n",
            config_path(&data)?,
            config_path(&root)?
        ),
    )?;
    let log = run_factorio(
        &executable,
        &config,
        &mods,
        &profile,
        &["--version"],
        "version",
        options.timeout,
    )?;
    check_version(&fs::read_to_string(log)?)?;
    run_factorio(
        &executable,
        &config,
        &mods,
        &profile,
        &[
            "--test-scenario",
            "factorium-verify",
            "--test-ticks",
            &ticks.to_string(),
        ],
        "verify",
        options.timeout,
    )?;
    let result = fs::read(root.join("script-output/factorium/result.json"))
        .context("Scenario ended without verification results")?;
    serde_json::from_slice(&result).context("Invalid game verification report")
}

/// Find Factorio via FACTORIO_BIN, PATH, or common Steam/standalone installs.
pub fn discover_factorio() -> Result<PathBuf> {
    if let Some(path) = env::var_os("FACTORIO_BIN") {
        let path = PathBuf::from(path);
        ensure!(
            path.is_file(),
            "FACTORIO_BIN does not point to a file: {}",
            path.display()
        );
        return Ok(path);
    }
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("PATH") {
        candidates.extend(
            env::split_paths(&path).map(|p| p.join(format!("factorio{}", env::consts::EXE_SUFFIX))),
        );
    }
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(root) = env::var_os(variable) {
            let root = PathBuf::from(root);
            candidates.push(root.join("Steam/steamapps/common/Factorio/bin/x64/factorio.exe"));
            candidates.push(root.join("Factorio/bin/x64/factorio.exe"));
        }
    }
    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        for relative in [
            ".steam/steam/steamapps/common/Factorio/bin/x64/factorio",
            ".local/share/Steam/steamapps/common/Factorio/bin/x64/factorio",
        ] {
            candidates.push(home.join(relative));
        }
    }
    candidates.push(PathBuf::from("/opt/factorio/bin/x64/factorio"));
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .context("Factorio not found; specify --factorio or set FACTORIO_BIN")
}

/// Best-effort profile discovery. An explicit --mods path handles custom configs.
pub fn discover_mod_directory(executable: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(root) = executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
    {
        candidates.push(root.join("mods"));
    }
    if let Some(appdata) = env::var_os("APPDATA") {
        candidates.push(PathBuf::from(appdata).join("Factorio/mods"));
    }
    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        candidates.push(home.join(".factorio/mods"));
    }
    candidates.into_iter().find(|path| path.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_and_unrecognized_versions() {
        assert!(check_version("Version: 2.1.24 (build 87808)\nVersion: 64").is_ok());
        assert!(check_version("Version: 2.0.72 (build 1)").is_err());
        assert!(check_version("Version: 2.1.23 (build 1)").is_err());
        assert!(check_version("not a Factorio executable").is_err());
        assert!(check_version("Version: 2").is_err());
    }

    #[test]
    fn snapshots_mods_and_settings_without_changing_originals() -> Result<()> {
        let source = tempfile::tempdir()?;
        let target = tempfile::tempdir()?;
        fs::create_dir(source.path().join("custom_1.0.0"))?;
        fs::write(source.path().join("custom_1.0.0/data.lua"), "original")?;
        fs::write(source.path().join("mod-settings.dat"), [0, 1, 255])?;
        copy_directory(source.path(), target.path())?;
        fs::write(target.path().join("custom_1.0.0/data.lua"), "changed")?;
        assert_eq!(
            fs::read_to_string(source.path().join("custom_1.0.0/data.lua"))?,
            "original"
        );
        assert_eq!(
            fs::read(target.path().join("mod-settings.dat"))?,
            [0, 1, 255]
        );
        Ok(())
    }
}
