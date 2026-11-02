import { invoke } from "@tauri-apps/api/core";
import { Download, Play, Sparkles, Square, X } from "lucide-react";
import React, { useEffect, useMemo, useState } from "react";
import { ContentBrowser } from "./components/ContentBrowser";
import { CrashReportModal } from "./components/CrashReportModal";
import { InstanceGrid } from "./components/InstanceGrid";
import { ModManagerModal } from "./components/ModManagerModal";
import { Settings } from "./components/Settings";
import { SetupEnvironmentInfo, SetupWizard } from "./components/SetupWizard";
import { SharePackModal } from "./components/SharePackModal";
import { Sidebar } from "./components/Sidebar";
import { VersionDownloader } from "./components/VersionDownloader";
import {
  InstanceConfig,
  UpdateCheckResult,
  useLauncher,
} from "./hooks/useLauncher";
import { usePluginBus } from "./hooks/usePluginBus";
import { Language } from "./i18n/translations";

export const App: React.FC = () => {
  const [activeTab, setActiveTab] = useState<
    "instances" | "store" | "downloader" | "settings"
  >("instances");
  const [modModalInstance, setModModalInstance] =
    useState<InstanceConfig | null>(null);
  const [storeLockedInstance, setStoreLockedInstance] =
    useState<InstanceConfig | null>(null);
  const [shareModalState, setShareModalState] = useState<{
    open: boolean;
    instance: InstanceConfig | null;
  }>({ open: false, instance: null });

  const [setupEnv, setSetupEnv] = useState<SetupEnvironmentInfo | null>(null);
  const [availableUpdate, setAvailableUpdate] =
    useState<UpdateCheckResult | null>(null);
  const [installingUpdate, setInstallingUpdate] = useState(false);

  const launcher = useLauncher();
  const pluginBus = usePluginBus(launcher.plugins);

  // Check if running as SkyLauncher-Setup.exe; otherwise check GitHub for updates on startup
  useEffect(() => {
    (async () => {
      try {
        const env = await invoke<SetupEnvironmentInfo>(
          "get_setup_environment_cmd"
        );
        setSetupEnv(env);
        if (!env.isSetupMode) {
          const upd = await invoke<UpdateCheckResult>("check_app_update_cmd", {
            customRepo: null,
          });
          if (upd && upd.updateAvailable) {
            setAvailableUpdate(upd);
          }
        }
      } catch {
        // ignore if offline or outside Tauri
      }
    })();
  }, []);

  // Disable right-click context menu and DevTools shortcuts globally
  useEffect(() => {
    const handleContextMenu = (e: MouseEvent) => {
      e.preventDefault();
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (
        e.key === "F12" ||
        (e.ctrlKey &&
          e.shiftKey &&
          (e.key === "I" ||
            e.key === "i" ||
            e.key === "J" ||
            e.key === "j" ||
            e.key === "C" ||
            e.key === "c")) ||
        (e.ctrlKey && (e.key === "U" || e.key === "u"))
      ) {
        e.preventDefault();
      }
    };
    window.addEventListener("contextmenu", handleContextMenu);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("contextmenu", handleContextMenu);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, []);

  const language: Language = useMemo(
    () => (launcher.globalSettings.language === "en" ? "en" : "tr"),
    [launcher.globalSettings.language]
  );

  const handleToggleLanguage = async (nextLang: Language) => {
    const updated = { ...launcher.globalSettings, language: nextLang };
    launcher.setGlobalSettings(updated);
    try {
      await invoke("save_global_settings", { settings: updated });
    } catch {
      // ignore if outside Tauri
    }
  };

  const handleToggleWindowSizeMode = async (mode: "small" | "medium") => {
    const updated = { ...launcher.globalSettings, windowSizeMode: mode };
    launcher.setGlobalSettings(updated);
    try {
      await invoke("set_window_size_mode", { mode });
      await invoke("save_global_settings", { settings: updated });
    } catch {
      // ignore if outside Tauri
    }
  };

  const activeLaunchOrRunningInst = useMemo(() => {
    const id =
      launcher.launchingInstanceId ||
      launcher.runningInstanceId ||
      launcher.downloadProgress?.instanceId;
    if (!id) return null;
    return launcher.instances.find((i) => i.id === id) || null;
  }, [
    launcher.launchingInstanceId,
    launcher.runningInstanceId,
    launcher.downloadProgress,
    launcher.instances,
  ]);

  const handleGlobalStopRunningGame = async () => {
    if (!launcher.runningInstanceId) return;
    try {
      await invoke("kill_instance_process_cmd", {
        instanceId: launcher.runningInstanceId,
      });
    } catch {}
    launcher.setRunningInstanceId(null);
  };

  const handleGlobalCancelLaunch = async () => {
    const targetId =
      launcher.launchingInstanceId || launcher.downloadProgress?.instanceId;
    if (targetId) {
      try {
        await invoke("cancel_download", { instanceId: targetId });
      } catch {}
    }
    launcher.setLaunchingInstanceId(null);
    launcher.setDownloadProgress(null);
  };

  const handleInstallStartupUpdate = async () => {
    if (!availableUpdate?.downloadUrl) return;
    setInstallingUpdate(true);
    try {
      await invoke("install_app_update_cmd", {
        downloadUrl: availableUpdate.downloadUrl,
        expectedSha256: null,
      });
    } catch {
      setInstallingUpdate(false);
    }
  };

  const handleInstanceCreated = (created: InstanceConfig) => {
    launcher.setInstances((prev) => [...prev, created]);
    pluginBus.emitInstanceDetected(created);
    setActiveTab("instances");
  };

  const handleQuickPlayWorld = async (
    inst: InstanceConfig,
    worldFolder: string
  ) => {
    launcher.setLaunchingInstanceId(inst.id);
    pluginBus.emitPreLaunch(inst);
    try {
      await invoke("launch_instance", {
        instanceId: inst.id,
        quickPlayWorld: worldFolder,
      });
    } catch {
      launcher.setLaunchingInstanceId(null);
    }
  };

  const handleCrashRecovered = async (
    updatedInstance: InstanceConfig,
    relaunch: boolean
  ) => {
    launcher.setInstances((prev) =>
      prev.map((i) => (i.id === updatedInstance.id ? updatedInstance : i))
    );
    launcher.setActiveCrashReport(null);
    if (relaunch) {
      launcher.setLaunchingInstanceId(updatedInstance.id);
      pluginBus.emitPreLaunch(updatedInstance);
      try {
        await invoke("launch_instance", {
          instanceId: updatedInstance.id,
          quickPlayWorld: null,
        });
      } catch {
        launcher.setLaunchingInstanceId(null);
      }
    }
  };

  if (setupEnv?.isSetupMode) {
    return (
      <SetupWizard
        envInfo={setupEnv}
        downloadProgress={launcher.downloadProgress}
        onExitSetupMode={() =>
          setSetupEnv((prev) => (prev ? { ...prev, isSetupMode: false } : null))
        }
      />
    );
  }

  return (
    <div className="flex w-screen h-screen bg-bg-primary text-text-primary overflow-hidden select-none">
      <Sidebar
        activeTab={activeTab}
        setActiveTab={setActiveTab}
        authStore={launcher.authStore}
        setAuthStore={launcher.setAuthStore}
        telemetry={launcher.telemetry}
        language={language}
        onToggleLanguage={handleToggleLanguage}
        windowSizeMode={launcher.globalSettings.windowSizeMode}
        onToggleWindowSizeMode={handleToggleWindowSizeMode}
      />

      <div className="flex-1 flex flex-col min-w-0 h-full overflow-hidden">
        {availableUpdate && (
          <div className="px-4 py-2.5 bg-gradient-to-r from-indigo-950/90 via-accent-primary/20 to-emerald-950/90 border-b border-accent-primary/35 flex items-center justify-between gap-3 shrink-0">
            <div className="flex items-center gap-2 min-w-0">
              <Sparkles className="w-4 h-4 text-accent-secondary shrink-0" />
              <span className="text-xs font-semibold text-white truncate">
                {language === "tr"
                  ? `Yeni SkyLauncher Güncellemesi Mevcut: v${availableUpdate.latestVersion} (${availableUpdate.releaseTitle})`
                  : `New SkyLauncher Update Available: v${availableUpdate.latestVersion} (${availableUpdate.releaseTitle})`}
              </span>
            </div>
            <div className="flex items-center gap-2 shrink-0">
              {availableUpdate.downloadUrl && (
                <button
                  disabled={installingUpdate}
                  onClick={handleInstallStartupUpdate}
                  className="px-3 py-1 rounded-xl bg-emerald-600 hover:bg-emerald-500 disabled:opacity-50 text-white text-xs font-bold flex items-center gap-1.5 transition"
                >
                  <Download className="w-3.5 h-3.5" />
                  <span>
                    {installingUpdate
                      ? language === "tr"
                        ? "İndiriliyor ve Kuruluyor..."
                        : "Updating..."
                      : language === "tr"
                        ? "Şimdi Güncelle ve Yeniden Başlat"
                        : "Update & Restart Now"}
                  </span>
                </button>
              )}
              <button
                onClick={() => setAvailableUpdate(null)}
                className="p-1 rounded-lg hover:bg-white/10 text-text-secondary hover:text-white"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            </div>
          </div>
        )}

        <div className="flex-1 min-h-0 overflow-y-auto">
          {activeTab === "instances" && (
            <InstanceGrid
              instances={launcher.instances}
              setInstances={launcher.setInstances}
              globalSettings={launcher.globalSettings}
              javaRuntimes={launcher.javaRuntimes}
              launchingInstanceId={launcher.launchingInstanceId}
              setLaunchingInstanceId={launcher.setLaunchingInstanceId}
              runningInstanceId={launcher.runningInstanceId}
              setRunningInstanceId={launcher.setRunningInstanceId}
              downloadProgress={launcher.downloadProgress}
              setDownloadProgress={launcher.setDownloadProgress}
              onOpenModManager={(inst) => setModModalInstance(inst)}
              onOpenShareModal={(inst) =>
                setShareModalState({ open: true, instance: inst })
              }
              onNavigateDownloader={() => setActiveTab("downloader")}
              onTriggerCrashReport={(rep) => {
                launcher.setActiveCrashReport(rep);
                pluginBus.emitGameCrash(rep);
              }}
              onPreLaunchHook={(inst) => pluginBus.emitPreLaunch(inst)}
              language={language}
            />
          )}

          {activeTab === "store" && (
            <ContentBrowser
              instances={launcher.instances}
              lockedInstance={storeLockedInstance}
              onSelectTargetInstance={setStoreLockedInstance}
              onModpackInstalled={handleInstanceCreated}
              language={language}
            />
          )}

          {activeTab === "downloader" && (
            <VersionDownloader
              onInstanceCreated={handleInstanceCreated}
              language={language}
            />
          )}

          {activeTab === "settings" && (
            <Settings
              globalSettings={launcher.globalSettings}
              setGlobalSettings={launcher.setGlobalSettings}
              themes={launcher.themes}
              onApplyTheme={launcher.applyThemeToDom}
              plugins={launcher.plugins}
              setPlugins={launcher.setPlugins}
              javaRuntimes={launcher.javaRuntimes}
              setJavaRuntimes={launcher.setJavaRuntimes}
              pluginBusLogs={pluginBus.busLogs}
              language={language}
              onToggleLanguage={handleToggleLanguage}
            />
          )}
        </div>

        {/* Unified Global Running & Launching Persistent Bottom Bar */}
        {(launcher.launchingInstanceId ||
          launcher.runningInstanceId ||
          launcher.downloadProgress) && (
          <div className="shrink-0 px-5 py-3 bg-bg-card/95 backdrop-blur border-t border-border-subtle shadow-2xl flex items-center justify-between gap-4 z-40">
            {/* Left: Instance identity and state */}
            <div className="flex items-center gap-3 min-w-0">
              {launcher.runningInstanceId ? (
                <div className="w-8 h-8 rounded-xl bg-emerald-500/20 border border-emerald-500/40 flex items-center justify-center shrink-0">
                  <span className="w-2.5 h-2.5 rounded-full bg-emerald-400 animate-ping" />
                </div>
              ) : (
                <div className="w-8 h-8 rounded-xl bg-accent-primary/20 border border-accent-primary/40 flex items-center justify-center shrink-0 text-accent-secondary">
                  <Play className="w-4 h-4 fill-current animate-pulse" />
                </div>
              )}

              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <span className="font-bold text-xs text-text-primary truncate">
                    {activeLaunchOrRunningInst?.name || "Minecraft"}
                  </span>
                  {activeLaunchOrRunningInst && (
                    <span className="text-[10px] text-text-muted px-1.5 py-0.5 rounded bg-bg-elevated font-mono">
                      {activeLaunchOrRunningInst.mcVersion} •{" "}
                      {activeLaunchOrRunningInst.modloader}
                    </span>
                  )}
                </div>
                <div className="text-[11px] text-text-secondary truncate mt-0.5">
                  {launcher.runningInstanceId
                    ? language === "tr"
                      ? "🟢 Oyun Çalışıyor (Minecraft Aktif)"
                      : "🟢 Game is running (Minecraft active)"
                    : launcher.downloadProgress?.stageText ||
                      (language === "tr"
                        ? "Aşama: Oyun hazırlanıyor ve başlatılıyor..."
                        : "Stage: Preparing and launching game...")}
                  {launcher.downloadProgress &&
                    launcher.downloadProgress.totalFiles > 0 && (
                      <span className="text-text-muted ml-1.5">
                        ({launcher.downloadProgress.currentLabel} —{" "}
                        {launcher.downloadProgress.completedFiles}/
                        {launcher.downloadProgress.totalFiles})
                      </span>
                    )}
                </div>
              </div>
            </div>

            {/* Center: Progress bar if launching */}
            {!launcher.runningInstanceId && launcher.downloadProgress && (
              <div className="flex-1 max-w-xs flex items-center gap-2.5">
                <div className="flex-1 h-2 rounded-full bg-bg-elevated overflow-hidden border border-border-subtle">
                  <div
                    className="h-full bg-accent-primary transition-all duration-150"
                    style={{
                      width: `${Math.round(
                        launcher.downloadProgress.totalFiles > 0
                          ? (launcher.downloadProgress.completedFiles /
                              launcher.downloadProgress.totalFiles) *
                              100
                          : 0
                      )}%`,
                    }}
                  />
                </div>
                <span className="text-xs font-semibold text-text-primary min-w-[36px] text-right font-mono">
                  %
                  {Math.round(
                    launcher.downloadProgress.totalFiles > 0
                      ? (launcher.downloadProgress.completedFiles /
                          launcher.downloadProgress.totalFiles) *
                          100
                      : 0
                  )}
                </span>
              </div>
            )}

            {/* Right: Actions */}
            <div className="flex items-center gap-2 shrink-0">
              {launcher.runningInstanceId ? (
                <button
                  onClick={handleGlobalStopRunningGame}
                  className="px-3.5 py-1.5 rounded-xl bg-red-600 hover:bg-red-500 text-white text-xs font-semibold flex items-center gap-1.5 shadow-sm transition"
                  title={
                    language === "tr" ? "Oyunu zorla sonlandır" : "Force close game"
                  }
                >
                  <Square className="w-3.5 h-3.5 fill-current" />
                  <span>
                    {language === "tr" ? "Oyunu Kapat / Durdur" : "Stop Game"}
                  </span>
                </button>
              ) : (
                <button
                  onClick={handleGlobalCancelLaunch}
                  className="px-3 py-1.5 rounded-xl bg-red-500/20 hover:bg-red-500/30 text-red-300 hover:text-white border border-red-500/30 text-xs font-semibold flex items-center gap-1.5 transition"
                  title={language === "tr" ? "Açılışı iptal et" : "Cancel launch"}
                >
                  <X className="w-3.5 h-3.5" />
                  <span>{language === "tr" ? "İptal Et" : "Cancel"}</span>
                </button>
              )}
            </div>
          </div>
        )}
      </div>

      {modModalInstance && (
        <ModManagerModal
          instance={modModalInstance}
          onClose={() => setModModalInstance(null)}
          onOpenStoreForInstance={(inst) => {
            setModModalInstance(null);
            setStoreLockedInstance(inst);
            setActiveTab("store");
          }}
          onQuickPlayWorld={handleQuickPlayWorld}
          language={language}
        />
      )}

      {shareModalState.open && (
        <SharePackModal
          instance={shareModalState.instance}
          onClose={() => setShareModalState({ open: false, instance: null })}
          onPackImported={handleInstanceCreated}
          language={language}
        />
      )}

      {launcher.activeCrashReport && (
        <CrashReportModal
          report={launcher.activeCrashReport}
          onClose={() => launcher.setActiveCrashReport(null)}
          onRecovered={handleCrashRecovered}
          language={language}
        />
      )}
    </div>
  );
};

export default App;
