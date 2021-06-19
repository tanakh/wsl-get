mod bindings;
mod wsl;

use std::{
    fs::{self, File},
    io::copy,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{anyhow, bail, Result};
use directories::BaseDirs;
use flate2::{write::GzEncoder, Compression};
use scopeguard::defer;
use tempfile::NamedTempFile;

use crate::wsl::WSL;

/// list installed distributions
#[argopt::subcmd]
fn list() -> Result<()> {
    let wsl = wsl::WSL::new();
    let distros = wsl.list_installed_distros()?;

    for distro in distros {
        // let conf = wsl.get_distribution_configuration(&distro)?;
        println!("{}", distro);
    }

    Ok(())
}

/// Install distribution
#[argopt::subcmd]
fn install(
    /// Do not add user after installation
    #[opt(long)]
    no_user: bool,
    ///
    /// Name of distribution to install (e.g. ubuntu, ubuntu:20.04)
    distro: String,
    ///
    /// Installing name
    install_name: Option<String>,
) -> Result<()> {
    let wsl = WSL::new();

    let base_dirs = BaseDirs::new().unwrap();

    let (distro_name, distro_tag) = parse_distro_name(&distro)?;

    let install_name =
        install_name.unwrap_or_else(|| format!("{}-{}", sanitize_path(&distro_name), distro_tag));

    if wsl.is_distribution_registered(&install_name) {
        bail!("Distribution `{}` is already registered", install_name);
    }

    println!("Installing {} as {}", distro, install_name);

    println!("Downloading rootfs image...",);

    let tar_gz = NamedTempFile::new()?;
    let tar_gz_path = tar_gz.into_temp_path();

    get_distribution_rootfs_tar_gz(&distro_name, &distro_tag, &tar_gz_path)?;

    println!("Registering distribution...",);

    let register_distro = || -> Result<()> {
        let distro_dir = base_dirs.cache_dir().join("wsl-get").join(&install_name);
        fs::create_dir_all(&distro_dir)?;
        wsl.register_distribution(&install_name, &distro_dir, &tar_gz_path)?;
        Ok(())
    };

    if no_user {
        register_distro()?;
    } else {
        let user_name: String = dialoguer::Input::new()
            .with_prompt("Enter new UNIX username")
            .interact_text()?;

        let password = dialoguer::Password::new()
            .with_prompt("New password")
            .with_confirmation("Retype new password", "Passwords do not match.")
            .interact()?;

        register_distro()?;

        wsl.create_user(&install_name, &user_name, &password)?;
        let uid = wsl.query_uid(&install_name, &user_name)?;

        let conf = wsl.get_distribution_configuration(&install_name)?;
        wsl.configure_distribution(&install_name, uid as _, conf.wsl_distribution_flags)?;
    }

    println!("Complete!");

    Ok(())
}

fn get_distribution_rootfs_tar_gz(distro: &str, tag: &str, path: &Path) -> Result<()> {
    println!("Pulling image...");

    let stat = Command::new("docker")
        .arg("pull")
        .arg(format!("{}:{}", distro, tag))
        .status()?;

    if !stat.success() {
        bail!("Failed to pull distribution: {}:{}", distro, tag);
    }

    println!("Exporting rootfs...");

    let output = Command::new("docker")
        .arg("create")
        .arg(format!("{}:{}", distro, tag))
        .output()?;

    if !output.status.success() {
        bail!("Failed to create container");
    }

    let id = String::from_utf8(output.stdout)?.trim().to_owned();

    defer! {
        let stat = Command::new("docker")
            .arg("rm")
            .arg(&id)
            .stderr(Stdio::inherit())
            .output()
            .unwrap();
        if !stat.status.success() {
            eprintln!("Failed to remove container");
        }
    }

    let mut temp_file = tempfile::NamedTempFile::new()?;

    let mut child = Command::new("docker")
        .arg("export")
        .arg(&id)
        .stdout(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.as_mut().unwrap();

    copy(
        stdout,
        &mut GzEncoder::new(File::create(&mut temp_file)?, Compression::fast()),
    )?;

    if !child.wait()?.success() {
        bail!("Failed to save distribution tarball");
    }

    temp_file.persist(path)?;

    Ok(())
}

fn parse_distro_name(distro: &str) -> Result<(String, String)> {
    let re = regex::Regex::new("^([^:]+)(:([^:]+))?$")?;

    let cap = re
        .captures(&distro)
        .ok_or_else(|| anyhow!("failed to parse distribution name"))?;

    let distro_name = &cap[1];
    let distro_tag = cap.get(3).map(|r| r.as_str()).unwrap_or("latest");

    Ok((distro_name.to_string(), distro_tag.to_string()))
}

fn sanitize_path(s: &str) -> String {
    s.chars().map(|c| if c == '/' { '-' } else { c }).collect()
}

/// Download tarball of rootfs
#[argopt::subcmd]
fn download(distro: String) -> Result<()> {
    let (distro_name, distro_tag) = parse_distro_name(&distro)?;

    let fname = PathBuf::from(format!(
        "{}-{}.tar.gz",
        sanitize_path(&distro_name),
        distro_tag
    ));
    get_distribution_rootfs_tar_gz(&distro_name, &distro_tag, &fname)?;
    println!("Saved rootfs to {}", fname.display());

    Ok(())
}

/// Set default user of distribution
#[argopt::subcmd(name = "set-default-user")]
fn set_default_user(distro: String, user_name: String) -> Result<()> {
    let wsl = WSL::new();

    let uid = wsl.query_uid(&distro, &user_name)?;
    let conf = wsl.get_distribution_configuration(&distro)?;
    wsl.configure_distribution(&distro, uid as _, conf.wsl_distribution_flags)?;

    Ok(())
}

/// Uninstall distribution
#[argopt::subcmd]
fn uninstall(
    /// Answer yes for all questions
    #[opt(long, short)]
    yes: bool,
    ///
    /// Name of distribution to uninstall
    distro: String,
) -> Result<()> {
    let wsl = WSL::new();

    let list = wsl.list_installed_distros()?;

    if !list.contains(&distro) {
        bail!("Distribution {} is not installed", distro);
    }

    if !yes
        && !dialoguer::Confirm::new()
            .with_prompt(format!("Do you really want to uninstall {}", distro))
            .interact()?
    {
        return Ok(());
    }

    println!("Uninstalling {}", distro);
    wsl.unregister_distribution(&distro)?;

    println!("Complete!");

    Ok(())
}

#[argopt::cmd_group(
    commands = [
        install,
        uninstall,
        set_default_user,
        list,
        download
    ]
)]
fn main() -> Result<()> {}
