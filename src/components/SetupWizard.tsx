import { invoke } from "@tauri-apps/api/core";
import {
  CheckCircle2,
  Cpu,
  Download,
  FolderOpen,
  Play,
  Rocket,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import React, { useState } from "react";
import type { DownloadProgressEvent } from "../hooks/useLauncher";

export interface SetupEnvironmentInfo {
  isSetupMode: boolean;
  defaultInstallDir: string;
  webview2Installed: boolean;
  installedJavaMajors: number[];
  githubRepo: string;
  appVersion: string;
}

interface SetupInstallResult {
  installedExePath: string;
  installedDir: string;
  downloadedFromGithub: boolean;
  java21Ready: boolean;
  shortcutsCreated: boolean;
}

interface SetupWizardProps {
  envInfo: SetupEnvironmentInfo;
  downloadProgress: DownloadProgressEvent | null;
  onExitSetupMode: () => void;
}

const OFFICIAL_GITHUB_REPO = "Skynzz/SkyLauncher";

export const SetupWizard: React.FC<SetupWizardProps> = ({
  envInfo,
  downloadProgress,
  onExitSetupMode,
}) => {
  const [installDir, setInstallDir] = useState(envInfo.defaultInstallDir);
  const [installJava21, setInstallJava21] = useState(true);
  const [createDesktopShortcut, setCreateDesktopShortcut] = useState(true);

  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<SetupInstallResult | null>(null);

  const hasJava21 = envInfo.installedJavaMajors.some((v) => v >= 21);

  const handleBrowseFolder = async () => {
    try {
      const picked = await invoke<string | null>("pick_folder_dialog_cmd", {
        initialDir: installDir,
      });
      if (picked) {
        const normalized =
          picked.endsWith("\\SkyLauncher") || picked.endsWith("/SkyLauncher")
            ? picked
            : `${picked.replace(/[\\/]+$/, "")}\\SkyLauncher`;
        setInstallDir(normalized);
      }
    } catch (err) {
      setError(String(err));
    }
  };

  const handleStartInstall = async () => {
    setInstalling(true);
    setError(null);
    try {
      const res = await invoke<SetupInstallResult>("run_setup_installer_cmd", {
        installDir,
        githubRepo: OFFICIAL_GITHUB_REPO,
        installJava21,
        createDesktopShortcut,
      });
      setResult(res);
    } catch (err) {
      setError(String(err));
    } finally {
      setInstalling(false);
    }
  };

  const handleLaunchInstalledApp = async () => {
    if (!result) return;
    try {
      await invoke("launch_installed_app_cmd", {
        exePath: result.installedExePath,
      });
    } catch {
      onExitSetupMode();
    }
  };

  const progressPct =
    downloadProgress && downloadProgress.totalFiles > 0
      ? Math.min(
          100,
          Math.round(
            (downloadProgress.completedFiles / downloadProgress.totalFiles) *
              100
          )
        )
      : 35;

  return (
    <div className="h-screen w-screen bg-bg-primary text-text-primary flex items-center justify-center p-6 select-none overflow-hidden">
      <div className="w-full max-w-2xl rounded-3xl bg-bg-card border border-border-subtle shadow-2xl overflow-hidden flex flex-col">
        {/* Top Banner */}
        <div className="p-6 bg-gradient-to-r from-indigo-950/80 via-bg-secondary to-sky-950/70 border-b border-border-subtle flex items-center justify-between">
          <div className="flex items-center gap-3.5">
            <div className="w-12 h-12 rounded-2xl bg-accent-primary/20 border border-accent-primary/40 flex items-center justify-center shadow-inner">
              <Rocket className="w-6 h-6 text-accent-secondary" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h1 className="text-lg font-extrabold tracking-tight text-white">
                  SkyLauncher Kurulum Sihirbazı
                </h1>
                <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-accent-primary/25 text-accent-secondary border border-accent-primary/30">
                  v{envInfo.appVersion} Setup
                </span>
              </div>
              <p className="text-xs text-text-secondary mt-0.5">
                Gerekli bağımlılıkları denetler, resmi bulut sunucusundan SkyLauncher'ı indirir ve kullanıma hazır kurar.
              </p>
            </div>
          </div>
          <div className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-emerald-500/10 border border-emerald-500/30 text-emerald-300 text-[11px] font-semibold">
            <ShieldCheck className="w-4 h-4" />
            <span>SkyShield Korumalı</span>
          </div>
        </div>

        {/* Body */}
        <div className="p-6 space-y-5 overflow-y-auto max-h-[75vh]">
          {!result ? (
            <>
              {/* Dependency Status Cards */}
              <div>
                <h2 className="text-xs font-bold uppercase tracking-wider text-text-secondary mb-2.5">
                  1. Sistem Bağımlılıkları ve Gereksinimler
                </h2>
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                  <div className="p-3.5 rounded-2xl bg-bg-secondary border border-border-subtle flex items-start gap-3">
                    <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0 mt-0.5" />
                    <div>
                      <div className="text-xs font-bold text-text-primary">
                        Microsoft WebView2 Runtime
                      </div>
                      <div className="text-[11px] text-emerald-400 font-medium mt-0.5">
                        Sistemde Kurulu ve Aktif
                      </div>
                    </div>
                  </div>

                  <div className="p-3.5 rounded-2xl bg-bg-secondary border border-border-subtle flex items-start gap-3">
                    <Cpu
                      className={`w-4 h-4 shrink-0 mt-0.5 ${
                        hasJava21 ? "text-emerald-400" : "text-amber-400"
                      }`}
                    />
                    <div className="flex-1">
                      <div className="text-xs font-bold text-text-primary">
                        Adoptium Java 21 JRE (OpenJDK)
                      </div>
                      <div className="text-[11px] text-text-secondary mt-0.5">
                        {hasJava21
                          ? `Kurulu (Algılanan: Java ${envInfo.installedJavaMajors.join(", ")})`
                          : "Eksik — Kurulum sırasında otomatik indirilecek"}
                      </div>
                      {!hasJava21 && (
                        <label className="mt-2 flex items-center gap-2 text-[11px] text-accent-secondary cursor-pointer">
                          <input
                            type="checkbox"
                            checked={installJava21}
                            onChange={(e) => setInstallJava21(e.target.checked)}
                            className="rounded border-border-subtle"
                          />
                          <span>Java 21 Runtime'ı otomatik indir ve kur</span>
                        </label>
                      )}
                    </div>
                  </div>
                </div>
              </div>

              {/* Installation Directory Picker */}
              <div>
                <label className="block text-xs font-bold uppercase tracking-wider text-text-secondary mb-2">
                  2. Kurulum Dosya Konumu
                </label>
                <div className="flex items-center gap-2">
                  <input
                    type="text"
                    value={installDir}
                    onChange={(e) => setInstallDir(e.target.value)}
                    disabled={installing}
                    placeholder="C:\Users\...\AppData\Local\Programs\SkyLauncher"
                    className="flex-1 px-3.5 py-2.5 rounded-xl bg-bg-secondary border border-border-subtle text-xs text-text-primary font-mono focus:outline-none focus:border-accent-primary"
                  />
                  <button
                    type="button"
                    onClick={handleBrowseFolder}
                    disabled={installing}
                    className="px-4 py-2.5 rounded-xl bg-bg-elevated hover:bg-bg-secondary border border-border-subtle text-xs font-semibold text-text-primary flex items-center gap-1.5 transition shrink-0"
                  >
                    <FolderOpen className="w-4 h-4 text-accent-secondary" />
                    <span>Gözat...</span>
                  </button>
                </div>
                <p className="text-[11px] text-text-secondary mt-1.5">
                  SkyLauncher çalıştırılabilir dosyası (<code>SkyLauncher.exe</code>) seçtiğiniz bu klasöre kurulacaktır. Harici bir programa (CurseForge vb.) ihtiyaç duymadan bağımsız çalışır.
                </p>
              </div>

              {/* Shortcut Option */}
              <div className="p-3.5 rounded-2xl bg-bg-secondary border border-border-subtle flex items-center justify-between">
                <label className="flex items-center gap-2.5 text-xs font-medium text-text-primary cursor-pointer">
                  <input
                    type="checkbox"
                    checked={createDesktopShortcut}
                    onChange={(e) => setCreateDesktopShortcut(e.target.checked)}
                    disabled={installing}
                    className="rounded border-border-subtle"
                  />
                  <span>
                    Masaüstü ve Başlat Menüsüne <strong>SkyLauncher</strong> kısayolu ekle
                  </span>
                </label>
                <span className="text-[10px] font-mono text-text-muted">
                  Kaynak: {OFFICIAL_GITHUB_REPO}
                </span>
              </div>

              {/* Live Progress Bar during Installation */}
              {installing && (
                <div className="p-4 rounded-2xl bg-accent-primary/10 border border-accent-primary/30 space-y-2">
                  <div className="flex items-center justify-between text-xs">
                    <span className="font-semibold text-accent-secondary">
                      {downloadProgress?.stageText ||
                        "Kurulum işlemleri yürütülüyor..."}
                    </span>
                    <span className="font-mono text-text-secondary">
                      %{progressPct}
                    </span>
                  </div>
                  <div className="w-full h-2 rounded-full bg-bg-secondary overflow-hidden">
                    <div
                      className="h-full bg-accent-primary transition-all duration-300"
                      style={{ width: `${progressPct}%` }}
                    />
                  </div>
                  {downloadProgress?.currentLabel && (
                    <div className="text-[11px] text-text-secondary font-mono truncate">
                      İşlenen: {downloadProgress.currentLabel}
                    </div>
                  )}
                </div>
              )}

              {error && (
                <div className="p-3.5 rounded-2xl bg-red-500/15 border border-red-500/35 text-xs text-red-200">
                  {error}
                </div>
              )}

              {/* Actions */}
              <div className="pt-2 flex items-center justify-between gap-3">
                <button
                  type="button"
                  onClick={onExitSetupMode}
                  disabled={installing}
                  className="px-4 py-2.5 rounded-xl bg-bg-secondary hover:bg-bg-elevated border border-border-subtle text-xs font-medium text-text-secondary hover:text-text-primary transition"
                >
                  Kurulumsuz Doğrudan Aç (Portable Mod)
                </button>
                <button
                  type="button"
                  onClick={handleStartInstall}
                  disabled={installing || !installDir.trim()}
                  className="px-6 py-2.5 rounded-xl bg-accent-primary hover:opacity-95 disabled:opacity-50 text-white text-xs font-bold flex items-center gap-2 shadow-lg transition"
                >
                  <Download className="w-4 h-4" />
                  <span>
                    {installing
                      ? "Kuruluyor, Lütfen Bekleyin..."
                      : "SkyLauncher'ı Şimdi Kur"}
                  </span>
                </button>
              </div>
            </>
          ) : (
            /* Completion Screen */
            <div className="py-6 text-center space-y-5">
              <div className="w-16 h-16 rounded-3xl bg-emerald-500/20 border border-emerald-500/40 flex items-center justify-center mx-auto">
                <Sparkles className="w-8 h-8 text-emerald-400" />
              </div>
              <div>
                <h2 className="text-xl font-extrabold text-white">
                  SkyLauncher Başarıyla Kuruldu!
                </h2>
                <p className="text-xs text-text-secondary mt-1">
                  Tüm bağımlılıklar doğrulandı ve uygulama kullanıma hazır hale getirildi.
                </p>
              </div>

              <div className="p-4 rounded-2xl bg-bg-secondary border border-border-subtle text-left space-y-2 text-xs">
                <div className="flex items-center justify-between">
                  <span className="text-text-secondary">Kurulum Konumu:</span>
                  <span className="font-mono text-text-primary">
                    {result.installedExePath}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-text-secondary">Kurulum Kaynağı:</span>
                  <span className="text-emerald-400 font-semibold">
                    {result.downloadedFromGithub
                      ? `Resmi GitHub Deposu (${OFFICIAL_GITHUB_REPO})`
                      : "Yerleşik Kurulum Paketi (Doğrulanmış PE)"}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-text-secondary">Java Runtime Durumu:</span>
                  <span className="text-emerald-400 font-semibold">
                    {result.java21Ready ? "Hazır (Java 21+)" : "Sistem Varsayılanı"}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-text-secondary">Kısayollar:</span>
                  <span className="text-text-primary">
                    {result.shortcutsCreated
                      ? "Masaüstü ve Başlat Menüsü"
                      : "Tamamlandı"}
                  </span>
                </div>
              </div>

              <div className="flex items-center justify-center gap-3 pt-2">
                <button
                  type="button"
                  onClick={handleLaunchInstalledApp}
                  className="px-7 py-3 rounded-2xl bg-emerald-600 hover:bg-emerald-500 text-white text-sm font-bold flex items-center gap-2 shadow-lg transition"
                >
                  <Play className="w-4 h-4 fill-current" />
                  <span>SkyLauncher'ı Başlat</span>
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
