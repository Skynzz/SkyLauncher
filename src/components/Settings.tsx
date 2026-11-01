import { invoke } from "@tauri-apps/api/core";
import {
  Check,
  Download,
  ExternalLink,
  RefreshCw,
  Rocket,
  Sparkles,
} from "lucide-react";
import React, { useState } from "react";
import type {
  DetectedJavaRuntime,
  GlobalSettings,
  PluginDescriptor,
  ThemeManifest,
  UpdateCheckResult,
} from "../hooks/useLauncher";
import { Language, translations } from "../i18n/translations";

interface SettingsProps {
  globalSettings: GlobalSettings;
  setGlobalSettings: React.Dispatch<React.SetStateAction<GlobalSettings>>;
  themes: ThemeManifest[];
  onApplyTheme: (theme: ThemeManifest) => void;
  plugins: PluginDescriptor[];
  setPlugins: React.Dispatch<React.SetStateAction<PluginDescriptor[]>>;
  javaRuntimes: DetectedJavaRuntime[];
  setJavaRuntimes: React.Dispatch<React.SetStateAction<DetectedJavaRuntime[]>>;
  pluginBusLogs: string[];
  language: Language;
  onToggleLanguage: (lang: Language) => void;
}

export const Settings: React.FC<SettingsProps> = ({
  globalSettings,
  setGlobalSettings,
  themes,
  onApplyTheme,
  javaRuntimes,
  setJavaRuntimes,
  language,
  onToggleLanguage,
}) => {
  const t = translations[language];
  const [savedNotice, setSavedNotice] = useState(false);
  const [installingJavaMajor, setInstallingJavaMajor] = useState<number | null>(
    null
  );

  const handleSaveGlobal = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      const saved = await invoke<GlobalSettings>("save_global_settings", {
        settings: globalSettings,
      });
      setGlobalSettings(saved);
      setSavedNotice(true);
      setTimeout(() => setSavedNotice(false), 2000);
    } catch {
      // ignore
    }
  };

  const handleSelectTheme = async (theme: ThemeManifest) => {
    onApplyTheme(theme);
    const updated = { ...globalSettings, activeThemeId: theme.id };
    setGlobalSettings(updated);
    try {
      await invoke("save_global_settings", { settings: updated });
    } catch {
      // ignore
    }
  };

  const handleInstallAdoptium = async (major: number) => {
    setInstallingJavaMajor(major);
    try {
      await invoke<string>("install_adoptium_runtime", { javaMajor: major });
      const refreshed = await invoke<DetectedJavaRuntime[]>(
        "get_detected_java_runtimes"
      );
      setJavaRuntimes(refreshed);
    } catch {
      // ignore
    } finally {
      setInstallingJavaMajor(null);
    }
  };

  return (
    <div className="flex-1 h-screen overflow-y-auto p-6">
      <div className="max-w-xl mx-auto space-y-5">
        <h2 className="text-lg font-bold text-text-primary">
          {t.settingsTitle}
        </h2>

        {/* GitHub Auto-Updater Infrastructure Card */}
        <GitHubUpdaterCard
          globalSettings={globalSettings}
          setGlobalSettings={setGlobalSettings}
          language={language}
        />

        <form
          onSubmit={handleSaveGlobal}
          className="p-5 rounded-2xl bg-bg-card border border-border-subtle space-y-5"
        >
          {/* Language & Theme */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-medium text-text-secondary mb-1.5">
                {t.languageLabel}
              </label>
              <div className="grid grid-cols-2 gap-1.5">
                <button
                  type="button"
                  onClick={() => onToggleLanguage("tr")}
                  className={`py-2 rounded-xl text-xs font-medium border transition ${
                    language === "tr"
                      ? "bg-accent-primary text-white border-accent-primary"
                      : "bg-bg-secondary text-text-secondary border-border-subtle"
                  }`}
                >
                  Türkçe
                </button>
                <button
                  type="button"
                  onClick={() => onToggleLanguage("en")}
                  className={`py-2 rounded-xl text-xs font-medium border transition ${
                    language === "en"
                      ? "bg-accent-primary text-white border-accent-primary"
                      : "bg-bg-secondary text-text-secondary border-border-subtle"
                  }`}
                >
                  English
                </button>
              </div>
            </div>

            <div>
              <label className="block text-xs font-medium text-text-secondary mb-1.5">
                {t.themesSection}
              </label>
              <select
                value={globalSettings.activeThemeId}
                onChange={(e) => {
                  const found = themes.find((th) => th.id === e.target.value);
                  if (found) handleSelectTheme(found);
                }}
                className="w-full px-3 py-2 rounded-xl bg-bg-secondary border border-border-subtle text-xs text-text-primary"
              >
                {themes.map((th) => (
                  <option key={th.id} value={th.id}>
                    {th.name}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {/* Default RAM Allocation */}
          <div className="space-y-2 pt-2 border-t border-border-subtle">
            <div className="flex justify-between text-xs">
              <span className="font-medium text-text-secondary">
                {t.globalRamSection}
              </span>
              <span className="font-semibold text-text-primary">
                {globalSettings.defaultMaxRamMb} MB (
                {(globalSettings.defaultMaxRamMb / 1024).toFixed(1)} GB)
              </span>
            </div>
            <input
              type="range"
              min={2048}
              max={16384}
              step={512}
              value={globalSettings.defaultMaxRamMb}
              onChange={(e) =>
                setGlobalSettings({
                  ...globalSettings,
                  defaultMaxRamMb: Number(e.target.value),
                })
              }
              className="w-full accent-sky-500"
            />
          </div>

          {/* CurseForge Directory */}
          <div className="space-y-1.5 pt-2 border-t border-border-subtle">
            <label className="block text-xs font-medium text-text-secondary">
              {t.cfWatcherSection}
            </label>
            <input
              type="text"
              value={globalSettings.curseforgeInstancesDir || ""}
              onChange={(e) =>
                setGlobalSettings({
                  ...globalSettings,
                  curseforgeInstancesDir: e.target.value || null,
                })
              }
              placeholder={t.cfDirPlaceholder}
              className="w-full px-3.5 py-2 rounded-xl bg-bg-secondary border border-border-subtle text-xs text-text-primary"
            />
          </div>

          {/* Toggles */}
          <div className="space-y-2 pt-1">
            <label className="flex items-center gap-2 text-xs font-medium text-text-primary cursor-pointer">
              <input
                type="checkbox"
                checked={globalSettings.hideLauncherOnLaunch}
                onChange={(e) =>
                  setGlobalSettings({
                    ...globalSettings,
                    hideLauncherOnLaunch: e.target.checked,
                  })
                }
                className="rounded accent-sky-500"
              />
              <span>{t.hideLauncherToggle}</span>
            </label>

            <label className="flex items-center gap-2 text-xs font-medium text-text-primary cursor-pointer">
              <input
                type="checkbox"
                checked={globalSettings.autoBackupWorlds ?? true}
                onChange={(e) =>
                  setGlobalSettings({
                    ...globalSettings,
                    autoBackupWorlds: e.target.checked,
                  })
                }
                className="rounded accent-sky-500"
              />
              <span>{t.autoBackupWorldsToggle}</span>
            </label>
          </div>

          <div className="flex items-center justify-between pt-2">
            {savedNotice ? (
              <span className="text-xs font-medium text-emerald-400 flex items-center gap-1">
                <Check className="w-3.5 h-3.5" />
                {t.savedToast}
              </span>
            ) : (
              <span />
            )}
            <button
              type="submit"
              className="px-5 py-2 rounded-xl bg-accent-primary hover:opacity-90 text-white text-xs font-semibold transition"
            >
              {t.saveSettingsBtn}
            </button>
          </div>
        </form>

        {/* Shared Mod Pool Hardlink Deduplicator Card */}
        <ModPoolOptimizerCard label={t.optimizeModPoolBtn} language={language} />

        {/* Simplified Java Status Card */}
        <div className="p-5 rounded-2xl bg-bg-card border border-border-subtle space-y-3">
          <div className="text-xs font-medium text-text-secondary">
            {t.javaRuntimesSection}
          </div>
          <div className="grid grid-cols-3 gap-2.5">
            {[8, 17, 21].map((major) => {
              const isInstalled = javaRuntimes.some(
                (r) => r.majorVersion === major
              );
              return (
                <div
                  key={major}
                  className="p-3 rounded-xl bg-bg-secondary border border-border-subtle flex items-center justify-between"
                >
                  <span className="text-xs font-semibold text-text-primary">
                    Java {major}
                  </span>
                  {isInstalled ? (
                    <span className="text-[11px] text-emerald-400 font-medium">
                      {t.installedJavaBadge}
                    </span>
                  ) : (
                    <button
                      type="button"
                      disabled={installingJavaMajor !== null}
                      onClick={() => handleInstallAdoptium(major)}
                      className="px-2.5 py-1 rounded-lg bg-bg-elevated hover:bg-accent-primary hover:text-white text-text-secondary text-[11px] font-medium flex items-center gap-1 transition"
                    >
                      <Download className="w-3 h-3" />
                      <span>
                        {installingJavaMajor === major
                          ? "..."
                          : t.installJavaBtn}
                      </span>
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
};

const GitHubUpdaterCard: React.FC<{
  globalSettings: GlobalSettings;
  setGlobalSettings: React.Dispatch<React.SetStateAction<GlobalSettings>>;
  language: Language;
}> = ({ globalSettings, setGlobalSettings, language }) => {
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateCheckResult | null>(null);
  const [errorText, setErrorText] = useState<string | null>(null);
  const [statusText, setStatusText] = useState<string | null>(null);

  const repoValue = globalSettings.githubUpdateRepo || "Sky_o/SkyLauncher";

  const handleCheckUpdate = async () => {
    setChecking(true);
    setErrorText(null);
    setStatusText(null);
    try {
      // Save repo setting first
      await invoke("save_global_settings", {
        settings: {
          ...globalSettings,
          githubUpdateRepo: repoValue,
        },
      });
      const res = await invoke<UpdateCheckResult>("check_app_update_cmd", {
        customRepo: repoValue,
      });
      setUpdateInfo(res);
      if (!res.updateAvailable) {
        setStatusText(
          language === "tr"
            ? `SkyLauncher güncel! (Mevcut sürüm: v${res.currentVersion})`
            : `SkyLauncher is up to date! (Current: v${res.currentVersion})`
        );
      }
    } catch (err) {
      setErrorText(String(err));
    } finally {
      setChecking(false);
    }
  };

  const handleInstallUpdate = async () => {
    if (!updateInfo?.downloadUrl) return;
    setInstalling(true);
    setErrorText(null);
    try {
      const msg = await invoke<string>("install_app_update_cmd", {
        downloadUrl: updateInfo.downloadUrl,
        expectedSha256: updateInfo.sha256 || null,
      });
      setStatusText(msg);
    } catch (err) {
      setErrorText(String(err));
      setInstalling(false);
    }
  };

  return (
    <div className="p-5 rounded-2xl bg-bg-card border border-border-subtle space-y-3.5">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <Sparkles className="w-4 h-4 text-accent-primary" />
          <h3 className="text-xs font-bold text-text-primary">
            {language === "tr"
              ? "Uygulama Güncellemeleri (GitHub OTA)"
              : "Launcher Updates (GitHub OTA)"}
          </h3>
        </div>
        <span className="px-2 py-0.5 rounded-md bg-bg-elevated text-[10px] font-mono text-text-secondary">
          v{updateInfo?.currentVersion || "0.1.0"}
        </span>
      </div>

      <div className="flex items-center gap-2">
        <input
          type="text"
          value={repoValue}
          onChange={(e) =>
            setGlobalSettings({
              ...globalSettings,
              githubUpdateRepo: e.target.value,
            })
          }
          placeholder="kullanici/SkyLauncher veya version.json URL"
          className="flex-1 px-3 py-2 rounded-xl bg-bg-secondary border border-border-subtle text-xs text-text-primary focus:outline-none focus:border-accent-primary"
        />
        <button
          type="button"
          disabled={checking || installing}
          onClick={handleCheckUpdate}
          className="px-3.5 py-2 rounded-xl bg-bg-elevated hover:bg-accent-primary hover:text-white text-xs font-semibold text-text-primary flex items-center gap-1.5 transition shrink-0"
        >
          <RefreshCw
            className={`w-3.5 h-3.5 ${checking ? "animate-spin" : ""}`}
          />
          <span>
            {checking
              ? language === "tr"
                ? "Denetleniyor..."
                : "Checking..."
              : language === "tr"
              ? "Güncellemeleri Denetle"
              : "Check for Updates"}
          </span>
        </button>
      </div>

      <p className="text-[11px] text-text-muted leading-relaxed">
        {language === "tr"
          ? "GitHub Releases (/releases/latest) veya doğrudan version.json bağlantısı üzerinden yeni SkyLauncher.exe sürümünü kontrol eder ve tek tıkla indirip günceller."
          : "Checks GitHub Releases (/releases/latest) or a raw version.json endpoint and hot-swaps SkyLauncher.exe in one click."}
      </p>

      {errorText && (
        <div className="p-3 rounded-xl bg-red-500/10 border border-red-500/25 text-xs text-red-300">
          {errorText}
        </div>
      )}

      {statusText && (
        <div className="p-3 rounded-xl bg-emerald-500/10 border border-emerald-500/25 text-xs text-emerald-300 flex items-center gap-2">
          <Check className="w-4 h-4 shrink-0" />
          <span>{statusText}</span>
        </div>
      )}

      {updateInfo && updateInfo.updateAvailable && (
        <div className="p-4 rounded-xl bg-accent-primary/10 border border-accent-primary/30 space-y-3">
          <div className="flex items-center justify-between gap-2">
            <div>
              <div className="text-xs font-bold text-text-primary">
                {updateInfo.releaseTitle} (v{updateInfo.latestVersion})
              </div>
              {updateInfo.publishedAt && (
                <div className="text-[10px] text-text-muted">
                  {updateInfo.publishedAt.slice(0, 10)}
                </div>
              )}
            </div>
            {updateInfo.htmlUrl && (
              <a
                href={updateInfo.htmlUrl}
                target="_blank"
                rel="noreferrer"
                className="text-[11px] text-accent-secondary hover:underline flex items-center gap-1"
              >
                <ExternalLink className="w-3 h-3" />
                <span>GitHub</span>
              </a>
            )}
          </div>

          {updateInfo.changelog && (
            <div className="p-2.5 rounded-lg bg-bg-primary/80 border border-border-subtle text-[11px] text-text-secondary max-h-32 overflow-y-auto whitespace-pre-wrap">
              {updateInfo.changelog}
            </div>
          )}

          {updateInfo.downloadUrl ? (
            <button
              type="button"
              disabled={installing}
              onClick={handleInstallUpdate}
              className="w-full py-2 px-4 rounded-xl bg-accent-primary hover:opacity-90 disabled:opacity-50 text-white text-xs font-semibold flex items-center justify-center gap-2 transition"
            >
              <Rocket className="w-3.5 h-3.5" />
              <span>
                {installing
                  ? language === "tr"
                    ? "Güncelleme İndiriliyor ve Kuruluyor..."
                    : "Downloading & Installing Update..."
                  : language === "tr"
                  ? "Şimdi Güncelle ve Yeniden Başlat"
                  : "Update & Restart Now"}
              </span>
            </button>
          ) : (
            <div className="text-[11px] text-amber-300">
              {language === "tr"
                ? "Bu sürümde doğrudan .exe dosyası bulunamadı, GitHub sayfasından indirebilirsiniz."
                : "No direct .exe asset found in this release; please download via GitHub."}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

const ModPoolOptimizerCard: React.FC<{ label: string; language: Language }> = ({
  label,
  language,
}) => {
  const [resultText, setResultText] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  const handleOptimize = async () => {
    setRunning(true);
    setResultText(null);
    try {
      const rep = await invoke<{ deduplicatedFiles: number; savedBytes: number }>(
        "optimize_mod_pool_cmd"
      );
      const mb = (rep.savedBytes / (1024 * 1024)).toFixed(1);
      setResultText(
        language === "tr"
          ? `${rep.deduplicatedFiles} kopya mod birleştirildi (${mb} MB disk alanı kazanıldı).`
          : `Deduplicated ${rep.deduplicatedFiles} mod files (${mb} MB saved).`
      );
    } catch (err) {
      setResultText(String(err));
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="p-5 rounded-2xl bg-bg-card border border-border-subtle flex flex-col gap-2.5">
      <div className="flex items-center justify-between gap-3">
        <span className="text-xs font-medium text-text-secondary">{label}</span>
        <button
          type="button"
          disabled={running}
          onClick={handleOptimize}
          className="px-3.5 py-1.5 rounded-xl bg-bg-elevated hover:bg-accent-primary hover:text-white text-xs font-semibold text-text-primary transition"
        >
          {running ? "..." : language === "tr" ? "Optimize Et" : "Optimize"}
        </button>
      </div>
      {resultText && (
        <div className="text-xs text-emerald-400 font-medium">{resultText}</div>
      )}
    </div>
  );
};
