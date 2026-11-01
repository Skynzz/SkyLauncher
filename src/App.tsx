import { invoke } from "@tauri-apps/api/core";
import { Download, Sparkles, X } from "lucide-react";
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
