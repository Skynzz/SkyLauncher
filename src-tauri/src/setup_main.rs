#![windows_subsystem = "windows"]

use std::path::PathBuf;
use std::process::Command;

/// Checks if Microsoft Edge WebView2 Runtime is present on Windows; if missing,
/// silently downloads and installs the official Evergreen Bootstrapper before opening the Tauri Setup UI.
fn ensure_webview2_runtime_installed() {
    let pf_x86 = std::env::var("ProgramFiles(x86)")
        .unwrap_or_else(|_| "C:\\Program Files (x86)".to_string());
    let wv2_dir = PathBuf::from(pf_x86)
        .join("Microsoft")
        .join("EdgeWebView")
        .join("Application");

    if wv2_dir.exists() {
        return;
    }

    // Fallback registry check via reg.exe
    if let Ok(out) = Command::new("reg")
        .args([
            "query",
            "HKLM\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
            "/v",
            "pv",
        ])
        .output()
    {
        if out.status.success() {
            return;
        }
    }

    // Download and install Microsoft Edge WebView2 Evergreen Bootstrapper
    let bootstrap_ps = "$tmp = Join-Path $env:TEMP 'MicrosoftEdgeWebview2Setup.exe'; \
        Invoke-WebRequest -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $tmp -UseBasicParsing; \
        Start-Process -FilePath $tmp -ArgumentList '/silent', '/install' -Wait; \
        Remove-Item $tmp -ErrorAction SilentlyContinue";
    let _ = Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", bootstrap_ps])
        .status();
}

fn main() {
    ensure_webview2_runtime_installed();
    sky_launcher_lib::run()
}
