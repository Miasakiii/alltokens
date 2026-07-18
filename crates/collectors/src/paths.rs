use std::path::PathBuf;

/// 获取所有可能的用户数据目录 (含 WSL Windows 路径)
pub fn home_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // 本机 Linux home
    if let Some(home) = dirs::home_dir() {
        dirs.push(home);
    }

    // WSL: 扫描 /mnt/c/Users/*, /mnt/d/Users/* 等
    #[cfg(target_os = "linux")]
    {
        for drive in ['c', 'd', 'e', 'f'] {
            let mnt = PathBuf::from(format!("/mnt/{drive}/Users"));
            if mnt.exists() {
                if let Ok(entries) = std::fs::read_dir(&mnt) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            // 跳过 Public, Default 等系统用户
                            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            if name == "Public" || name == "Default" || name == "Default User" || name == "All Users" {
                                continue;
                            }
                            dirs.push(path);
                        }
                    }
                }
            }
        }
    }

    dirs
}

/// 在多个 base 目录下查找候选路径
pub fn find_paths(candidates: &[&str]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for base in home_dirs() {
        for candidate in candidates {
            let path = base.join(candidate);
            if path.exists() {
                found.push(path);
            }
        }
    }
    found
}

/// 检查 WSL 环境
pub fn is_wsl() -> bool {
    #[cfg(target_os = "linux")]
    {
        // 检查 /proc/version 是否包含 Microsoft
        if let Ok(version) = std::fs::read_to_string("/proc/version") {
            return version.to_lowercase().contains("microsoft") || version.to_lowercase().contains("wsl");
        }
        // 检查 /mnt/c 是否存在
        return std::path::Path::new("/mnt/c").exists();
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}
