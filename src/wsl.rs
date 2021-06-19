use std::{
    cell::RefCell,
    convert::TryInto,
    ffi::CStr,
    path::Path,
    process::{Command, Stdio},
    ptr::null_mut,
    slice,
};

use crate::bindings::Windows::Win32::{
    Foundation::{BOOL, HANDLE, HINSTANCE, PSTR, PWSTR},
    System::{
        Com::CoTaskMemFree,
        LibraryLoader::{
            FreeLibrary, GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
        },
        SubsystemForLinux::WSL_DISTRIBUTION_FLAGS,
    },
};
use anyhow::{bail, Result};
use scopeguard::defer;
use windows::{IntoParam, HRESULT};

pub struct WSL {
    dll: HINSTANCE,

    configure_distribution: unsafe extern "system" fn(
        distributionname: PWSTR,
        defaultuid: u32,
        wsldistributionflags: WSL_DISTRIBUTION_FLAGS,
    ) -> ::windows::HRESULT,

    get_distribution_configuration: unsafe extern "system" fn(
        distributionname: PWSTR,
        distributionversion: *mut u32,
        defaultuid: *mut u32,
        wsldistributionflags: *mut WSL_DISTRIBUTION_FLAGS,
        defaultenvironmentvariables: *mut *mut PSTR,
        defaultenvironmentvariablecount: *mut u32,
    ) -> HRESULT,

    launch_interactive: unsafe extern "system" fn(
        distributionname: PWSTR,
        command: PWSTR,
        usecurrentworkingdirectory: BOOL,
        exitcode: *mut u32,
    ) -> ::windows::HRESULT,

    is_distribution_registered: unsafe extern "system" fn(distributionname: PWSTR) -> BOOL,

    unregister_distribution:
        unsafe extern "system" fn(distributionname: PWSTR) -> ::windows::HRESULT,
    // register_distribution: unsafe extern "system" fn(
    //     distributionname: PWSTR,
    //     targzfilename: PWSTR,
    // ) -> ::windows::HRESULT,
}

#[derive(Debug)]
pub struct DistributionConfiguration {
    pub distribution_version: u32,
    pub default_uid: u32,
    pub wsl_distribution_flags: WSL_DISTRIBUTION_FLAGS,
    pub default_environment_variables: Vec<String>,
}

impl WSL {
    pub fn new() -> Self {
        let dll =
            unsafe { LoadLibraryExW("wslapi.dll", HANDLE::NULL, LOAD_LIBRARY_SEARCH_SYSTEM32) };

        Self {
            dll,
            configure_distribution: unsafe {
                std::mem::transmute(GetProcAddress(dll, "WslConfigureDistribution"))
            },
            get_distribution_configuration: unsafe {
                std::mem::transmute(GetProcAddress(dll, "WslGetDistributionConfiguration"))
            },
            launch_interactive: unsafe {
                std::mem::transmute(GetProcAddress(dll, "WslLaunchInteractive"))
            },
            is_distribution_registered: unsafe {
                std::mem::transmute(GetProcAddress(dll, "WslIsDistributionRegistered"))
            },
            unregister_distribution: unsafe {
                std::mem::transmute(GetProcAddress(dll, "WslUnregisterDistribution"))
            },
            // register_distribution: unsafe {
            //     std::mem::transmute(GetProcAddress(dll, "WslRegisterDistribution"))
            // },
        }
    }

    // workaround for missing enumerate API
    pub fn list_installed_distros(&self) -> Result<Vec<String>> {
        let output = Command::new("wsl.exe")
            .arg("--list")
            .arg("--quiet")
            .output()?;

        Ok(decode_utf16(&output.stdout)?
            .lines()
            .map(|w| w.trim_end().to_string())
            .collect::<Vec<String>>())
    }

    pub fn configure_distribution(
        &self,
        distribution_name: &str,
        default_uid: u32,
        wsl_distribution_flags: WSL_DISTRIBUTION_FLAGS,
    ) -> Result<()> {
        Ok(unsafe {
            (self.configure_distribution)(
                IntoParam::<PWSTR>::into_param(distribution_name).abi(),
                default_uid,
                wsl_distribution_flags,
            )
        }
        .ok()?)
    }

    pub fn get_distribution_configuration(
        &self,
        distribution_name: &str,
    ) -> Result<DistributionConfiguration> {
        let mut distributionversion = 0;
        let mut defaultuid = 0;
        let mut wsldistributionflags = WSL_DISTRIBUTION_FLAGS::default();
        let mut defaultenvironmentvariables = null_mut();
        let mut defaultenvironmentvariablecount = 0;

        unsafe {
            (self.get_distribution_configuration)(
                IntoParam::<PWSTR>::into_param(distribution_name).abi(),
                &mut distributionversion,
                &mut defaultuid,
                &mut wsldistributionflags,
                &mut defaultenvironmentvariables,
                &mut defaultenvironmentvariablecount,
            )
        }
        .ok()?;

        let s = unsafe {
            slice::from_raw_parts_mut(
                defaultenvironmentvariables,
                defaultenvironmentvariablecount as usize,
            )
        };

        let env_vars = (0..defaultenvironmentvariablecount)
            .map(|i| {
                let s = unsafe { CStr::from_ptr(s[i as usize].0 as _) };
                s.to_string_lossy().to_string()
            })
            .collect::<Vec<String>>();

        unsafe {
            for i in 0..defaultenvironmentvariablecount {
                CoTaskMemFree(s[i as usize].0 as _);
            }
            CoTaskMemFree(defaultenvironmentvariables as _);
        }

        Ok(DistributionConfiguration {
            distribution_version: distributionversion,
            default_uid: defaultuid,
            wsl_distribution_flags: wsldistributionflags,
            default_environment_variables: env_vars,
        })
    }

    pub fn launch_interactive(
        &self,
        distribution_name: &str,
        command: &str,
        use_current_working_directory: bool,
    ) -> Result<u32> {
        let mut exitcode = 0;
        unsafe {
            (self.launch_interactive)(
                IntoParam::<PWSTR>::into_param(distribution_name).abi(),
                IntoParam::<PWSTR>::into_param(command).abi(),
                IntoParam::<BOOL>::into_param(use_current_working_directory).abi(),
                &mut exitcode,
            )
        }
        .ok()?;
        Ok(exitcode)
    }

    pub fn is_distribution_registered(&self, distribution_name: &str) -> bool {
        unsafe {
            (self.is_distribution_registered)(
                IntoParam::<PWSTR>::into_param(distribution_name).abi(),
            )
        }
        .as_bool()
    }

    pub fn unregister_distribution(&self, distribution_name: &str) -> Result<()> {
        Ok(unsafe {
            (self.unregister_distribution)(IntoParam::<PWSTR>::into_param(distribution_name).abi())
        }
        .ok()?)
    }

    pub fn register_distribution(
        &self,
        distribution_name: &str,
        data_dir: &Path,
        targz_filename: &Path,
    ) -> Result<()> {
        // Ok(unsafe {
        //     (self.register_distribution)(
        //         IntoParam::<PWSTR>::into_param(distribution_name).abi(),
        //         IntoParam::<PWSTR>::into_param(targz_filename).abi(),
        //     )
        // }
        // .ok()?)

        // WslRegisterDistribution always use the directory
        // same as executable.
        // This API is not suitable for this program,
        // so it use `wsl.exe --import` command.

        let stat = Command::new("wsl.exe")
            .arg("--import")
            .arg(distribution_name)
            .arg(data_dir)
            .arg(targz_filename)
            .args(&["--version", "2"])
            .status()?;

        if !stat.success() {
            bail!("Failed to register distribution");
        }

        Ok(())
    }

    pub fn create_user(&self, distro_name: &str, user_name: &str, password: &str) -> Result<()> {
        let bash_path = self.lookup_shell(distro_name)?;

        let mut user_add_args = vec![];
        user_add_args.push("/usr/sbin/useradd".to_owned());
        if let Some(bash_path) = bash_path {
            user_add_args.push("-s".to_owned());
            user_add_args.push(bash_path);
        }
        user_add_args.push("-m".to_owned());
        user_add_args.push(user_name.to_owned());

        let ec = self.launch_interactive(distro_name, &user_add_args.join(" "), true)?;
        if ec != 0 {
            bail!("Failed to add user.");
        }

        let complete = RefCell::new(false);

        defer! {
            if !*complete.borrow() {
                self.launch_interactive(
                    distro_name,
                    &format!("/usr/sbin/userdel --remove {}", user_name),
                    true,
                ).unwrap();
            }
        }

        let change_password = |user, pass| {
            let ec = self.launch_interactive(
                distro_name,
                &format!("echo {}:{} | /usr/sbin/chpasswd", user, pass),
                true,
            )?;
            if ec != 0 {
                bail!("Failed to change password.");
            }
            Ok(())
        };

        change_password("root", password)?;
        change_password(user_name, password)?;

        let add_group_if_exists = |group: &str| {
            self.launch_interactive(
                distro_name,
                &format!(
                    "getent group {} > /dev/null && /usr/sbin/usermod -aG {} {}",
                    group, group, user_name
                ),
                true,
            )
        };

        add_group_if_exists("wheel")?;
        add_group_if_exists("sudo")?;

        *complete.borrow_mut() = true;

        Ok(())
    }

    pub fn file_exists(&self, distro_name: &str, file: &str) -> Result<bool> {
        let ec =
            self.launch_interactive(distro_name, &format!("/usr/bin/test -e {}", file), true)?;
        Ok(ec == 0)
    }

    pub fn lookup_shell(&self, distro_name: &str) -> Result<Option<String>> {
        let shells = &["/usr/bin/bash", "/bin/bash", "/usr/bin/sh", "/bin/sh"];

        for &cand in shells {
            if self.file_exists(distro_name, cand)? {
                return Ok(Some(cand.to_string()));
            }
        }
        Ok(None)
    }

    pub fn query_uid(&self, distro_name: &str, user_name: &str) -> Result<u64> {
        let output = Command::new("wsl.exe")
            .arg("-d")
            .arg(distro_name)
            .arg("--")
            .args(&["/usr/bin/id", "-u", &user_name])
            .stderr(Stdio::piped())
            .output()?;

        if !output.status.success() {
            bail!("Failed to get uid");
        }

        Ok(String::from_utf8(output.stdout)?.trim().parse()?)
    }
}

impl Drop for WSL {
    fn drop(&mut self) {
        let _ = unsafe { FreeLibrary(self.dll) };
    }
}

fn decode_utf16(bytes: &[u8]) -> Result<String> {
    let output = bytes
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes(bytes.try_into().unwrap()))
        .collect::<Vec<_>>();

    Ok(String::from_utf16(&output)?)
}
