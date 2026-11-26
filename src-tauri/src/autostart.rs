#[cfg(target_os = "windows")]
use crate::cmd_util::hidden_command;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
fn macos_plist_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("LaunchAgents")
            .join("com.wifibuddy.app.plist"),
    )
}

pub fn is_autostart_enabled() -> bool {
    #[cfg(target_os = "macos")]
    {
        return macos_plist_path().map(|p| p.exists()).unwrap_or(false);
    }

    #[cfg(target_os = "windows")]
    {
        return hidden_command("reg")
            .args([
                "query",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                "/v",
                "WiFiBuddy",
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    false
}

pub fn set_autostart_enabled(enabled: bool) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        let path = macos_plist_path().ok_or_else(|| "无法定位 macOS LaunchAgents 目录".to_string())?;
        if enabled {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建 LaunchAgents 目录失败: {}", e))?;
            }
            let exe = std::env::current_exe()
                .map_err(|e| format!("获取当前程序路径失败: {}", e))?;
            let exe_str = exe.to_string_lossy();
            let plist = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.wifibuddy.app</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#,
                exe_str
            );
            std::fs::write(&path, plist).map_err(|e| format!("写入开机自启配置失败: {}", e))?;
            Ok(true)
        } else {
            if path.exists() {
                std::fs::remove_file(&path)
                    .map_err(|e| format!("移除开机自启配置失败: {}", e))?;
            }
            Ok(false)
        }
    }

    #[cfg(target_os = "windows")]
    {
        let key = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run";
        if enabled {
            let exe = std::env::current_exe()
                .map_err(|e| format!("获取当前程序路径失败: {}", e))?;
            let quoted = format!("\"{}\"", exe.to_string_lossy());
            let out = hidden_command("reg")
                .args(["add", key, "/v", "WiFiBuddy", "/t", "REG_SZ", "/d", &quoted, "/f"])
                .output()
                .map_err(|e| format!("写入 Windows 注册表失败: {}", e))?;
            if out.status.success() {
                Ok(true)
            } else {
                Err("设置 Windows 开机自启失败".to_string())
            }
        } else {
            let _ = hidden_command("reg")
                .args(["delete", key, "/v", "WiFiBuddy", "/f"])
                .output();
            Ok(false)
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = enabled;
        Err("当前系统暂不支持开机自启设置".to_string())
    }
}
