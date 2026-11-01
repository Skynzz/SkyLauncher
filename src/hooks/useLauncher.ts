import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";

export type ModloaderType = "vanilla" | "fabric" | "quilt" | "neoforge" | "forge";
export type InstanceSource = "custom" | "curseforge";

export interface OfflineProfile {
  id: string;
  username: string;
  uuid: string;
  accessToken: string;
  userType: string;
  avatarColor: string;
  createdAt: string;
  lastUsed: string;
}

export interface AuthStore {
  activeProfileId: string | null;
  profiles: OfflineProfile[];
}

export interface GlobalSettings {
  defaultMinRamMb: number;
  defaultMaxRamMb: number;
  defaultJavaPath: string | null;
  defaultJvmArgs: string[];
  curseforgeInstancesDir: string | null;
  hideLauncherOnLaunch: boolean;
  language: "tr" | "en";
  activeThemeId: string;
  curseforgeApiKey?: string | null;
  autoBackupWorlds?: boolean;
  githubUpdateRepo?: string;
}

export interface InstanceConfig {
  id: string;
  name: string;
  mcVersion: string;
  modloader: ModloaderType;
  modloaderVersion: string | null;
  source: InstanceSource;
  gameDir: string;
  iconColor: string;
  iconDataUrl?: string | null;
  overrideMemory: boolean;
  minRamMb: number;
  maxRamMb: number;
  overrideJava: boolean;
  javaPath: string | null;
  customJvmArgs: string[];
  createdAt: string;
  lastPlayed: string | null;
  playtimeMinutes?: number;
}

export interface MojangVersionEntry {
  id: string;
  type: string;
  url: string;
  time: string;
  releaseTime: string;
  sha1?: string;
}

export interface MojangManifest {
  latest: { release: string; snapshot: string };
  versions: MojangVersionEntry[];
}

export interface ModloaderVersionItem {
  loaderType: ModloaderType;
  loaderVersion: string;
  mcVersion: string;
  stable: boolean;
}

export interface ModMetadata {
  fileName: string;
  enabled: boolean;
  modId: string;
  name: string;
  version: string;
  description: string;
  authors: string[];
  loader: string;
  dependencies: string[];
  mixinConfigs: string[];
  fileSizeBytes: number;
}

export interface PackResourceEntry {
  fileName: string;
  category: string;
  sizeBytes: number;
  enabled: boolean;
}

export interface DetectedJavaRuntime {
  path: string;
  majorVersion: number;
  vendor: string;
}

export interface EffectiveLaunchSettings {
  minRamMb: number;
  maxRamMb: number;
  requiredJavaMajor: number;
  resolvedJavaPath: string;
  jvmFlags: string[];
}

export interface RecoveryAction {
  actionType:
    | "increase_ram"
    | "switch_java"
    | "reset_config"
    | "disable_mod"
    | "install_missing_mod";
  labelTr: string;
  labelEn: string;
  targetValue: string;
}

export interface CrashAnalysisReport {
  instanceId: string;
  exitCode: number;
  category:
    | "missing_dependency"
    | "mixin_conflict"
    | "out_of_memory"
    | "java_version_mismatch"
    | "corrupted_config"
    | "unknown";
  titleTr: string;
  titleEn: string;
  summaryTr: string;
  summaryEn: string;
  culpritModJar: string | null;
  missingModId: string | null;
  requiredVersionRange: string | null;
  failingMixinJson: string | null;
  corruptedConfigFile: string | null;
  requiredJavaVersion: number | null;
  recoveryActions: RecoveryAction[];
  relevantLogLines: string[];
  analyzedAt: string;
}

export interface ThemeManifest {
  id: string;
  name: string;
  author: string;
  description: string;
  variables: Record<string, string>;
  customCss: string;
}

export interface PluginDescriptor {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  enabled: boolean;
  hooks: string[];
  scriptCode: string;
}

export interface SystemTelemetry {
  launcherRamMb: number;
  systemTotalRamMb: number;
  appDataPath: string;
}

export interface DownloadProgressEvent {
  instanceId: string;
  completedFiles: number;
  totalFiles: number;
  currentLabel: string;
  phase: string;
  stageText: string;
}

export type StoreContentType = "mod" | "modpack" | "resourcepack" | "shader";
export type StoreProviderType = "modrinth" | "curseforge" | "both";

export interface StoreSearchItem {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  author: string;
  iconUrl: string | null;
  downloads: number;
  provider: string;
  contentType: StoreContentType;
  categories: string[];
  updatedAt: string;
}

export interface StoreDependencyBadge {
  projectId: string;
  dependencyType: "required" | "optional";
}

export interface StoreVersionEntry {
  versionId: string;
  versionNumber: string;
  name: string;
  fileName: string;
  downloadUrl: string;
  sha1: string | null;
  sizeBytes: number;
  gameVersions: string[];
  loaders: string[];
  dependencies: StoreDependencyBadge[];
}

export interface StoreProjectDetails {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  body: string;
  author: string;
  iconUrl: string | null;
  galleryUrls: string[];
  provider: string;
  contentType: StoreContentType;
  downloads: number;
  followers: number;
  updatedAt: string;
  license: string | null;
  sourceUrl: string | null;
  issuesUrl: string | null;
  wikiUrl: string | null;
  categories: string[];
  gameVersions: string[];
  loaders: string[];
  versions: StoreVersionEntry[];
}

export interface ModUpdateInfo {
  fileName: string;
  currentSha1: string;
  projectId: string;
  latestVersionId: string;
  latestVersionNumber: string;
  latestFileName: string;
  downloadUrl: string;
  latestSha1: string | null;
}

export interface SkyCodeExportResult {
  shareCode: string;
  savedFilePath: string | null;
  totalMods: number;
  resolvedOnlineMods: number;
}

export interface ModPoolOptimizationReport {
  deduplicatedFiles: number;
  savedBytes: number;
}

export interface WorldSaveEntry {
  folderName: string;
  displayName: string;
  sizeBytes: number;
  lastModified: string;
}

export interface WorldBackupEntry {
  fileName: string;
  worldName: string;
  sizeBytes: number;
  createdAt: string;
}

export interface ScreenshotEntry {
  fileName: string;
  dataUrl: string;
  sizeBytes: number;
}

export interface UpdateCheckResult {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  releaseTitle: string;
  changelog: string;
  publishedAt: string;
  downloadUrl: string | null;
  htmlUrl: string | null;
  sha256: string | null;
}

const isTauriRuntime = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function useLauncher(callbacks?: {
  onInstanceDetected?: (inst: InstanceConfig) => void;
  onGameLog?: (line: string) => void;
  onGameCrash?: (report: CrashAnalysisReport) => void;
}) {
  const [authStore, setAuthStore] = useState<AuthStore>({
    activeProfileId: "b50ad385-829d-3141-a216-7e7d7539ba7f",
    profiles: [
      {
        id: "b50ad385-829d-3141-a216-7e7d7539ba7f",
        username: "Developer",
        uuid: "b50ad385-829d-3141-a216-7e7d7539ba7f",
        accessToken: "0",
        userType: "legacy",
        avatarColor: "#6366f1",
        createdAt: new Date().toISOString(),
        lastUsed: new Date().toISOString(),
      },
    ],
  });

  const [instances, setInstances] = useState<InstanceConfig[]>([]);
  const [globalSettings, setGlobalSettings] = useState<GlobalSettings>({
    defaultMinRamMb: 1024,
    defaultMaxRamMb: 4096,
    defaultJavaPath: null,
    defaultJvmArgs: [],
    curseforgeInstancesDir: null,
    hideLauncherOnLaunch: false,
    language: "tr",
    activeThemeId: "sky-slate",
    curseforgeApiKey: null,
    autoBackupWorlds: true,
    githubUpdateRepo: "Skynzz/SkyLauncher",
  });

  const [themes, setThemes] = useState<ThemeManifest[]>([]);
  const [plugins, setPlugins] = useState<PluginDescriptor[]>([]);
  const [javaRuntimes, setJavaRuntimes] = useState<DetectedJavaRuntime[]>([]);
  const [telemetry, setTelemetry] = useState<SystemTelemetry>({
    launcherRamMb: 14.8,
    systemTotalRamMb: 16384,
    appDataPath: "%LOCALAPPDATA%\\SkyLauncher",
  });

  const [downloadProgress, setDownloadProgress] =
    useState<DownloadProgressEvent | null>(null);
  const [launchingInstanceId, setLaunchingInstanceId] = useState<string | null>(
    null
  );
  const [runningInstanceId, setRunningInstanceId] = useState<string | null>(
    null
  );
  const [activeCrashReport, setActiveCrashReport] =
    useState<CrashAnalysisReport | null>(null);

  const applyThemeToDom = useCallback((theme: ThemeManifest) => {
    const root = document.documentElement;
    for (const [k, v] of Object.entries(theme.variables)) {
      root.style.setProperty(k, v);
    }
    let styleTag = document.getElementById("aether-dynamic-theme-css");
    if (!styleTag) {
      styleTag = document.createElement("style");
      styleTag.id = "aether-dynamic-theme-css";
      document.head.appendChild(styleTag);
    }
    styleTag.textContent = theme.customCss || "";
  }, []);

  const refreshAll = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const [auth, instList, settings, themeList, pluginList, runtimes, telem] =
        await Promise.all([
          invoke<AuthStore>("get_auth_store"),
          invoke<InstanceConfig[]>("get_instances"),
          invoke<GlobalSettings>("get_global_settings"),
          invoke<ThemeManifest[]>("get_themes"),
          invoke<PluginDescriptor[]>("get_plugins"),
          invoke<DetectedJavaRuntime[]>("get_detected_java_runtimes"),
          invoke<SystemTelemetry>("get_system_telemetry"),
        ]);

      setAuthStore(auth);
      setInstances(instList);
      setGlobalSettings(settings);
      setThemes(themeList);
      setPlugins(pluginList);
      setJavaRuntimes(runtimes);
      setTelemetry(telem);

      const currentTheme =
        themeList.find((t) => t.id === settings.activeThemeId) || themeList[0];
      if (currentTheme) {
        applyThemeToDom(currentTheme);
      }
    } catch (err) {
      console.error("Failed to load initial state from Tauri backend:", err);
    }
  }, [applyThemeToDom]);

  useEffect(() => {
    refreshAll();
  }, [refreshAll]);

  useEffect(() => {
    if (!isTauriRuntime()) return;

    const unlistenPromises = [
      listen<DownloadProgressEvent>("download-progress", (ev) => {
        setDownloadProgress(ev.payload);
      }),
      listen<string>("download-finished", () => {
        setDownloadProgress(null);
      }),
      listen<string>("game-started", (ev) => {
        setLaunchingInstanceId(null);
        setRunningInstanceId(ev.payload);
      }),
      listen<[string, number]>("playtime-recorded", (ev) => {
        const [instId, mins] = ev.payload;
        invoke<InstanceConfig>("record_playtime_cmd", {
          instanceId: instId,
          addedMinutes: mins,
        })
          .then((updated) => {
            setInstances((prev) =>
              prev.map((i) => (i.id === updated.id ? updated : i))
            );
          })
          .catch(() => {});
      }),
      listen<InstanceConfig>("curseforge-instance-detected", (ev) => {
        setInstances((prev) => {
          const idx = prev.findIndex((i) => i.id === ev.payload.id);
          if (idx >= 0) {
            const copy = [...prev];
            copy[idx] = ev.payload;
            return copy;
          }
          return [...prev, ev.payload];
        });
        callbacks?.onInstanceDetected?.(ev.payload);
      }),
      listen<[string, string]>("game-log-line", (ev) => {
        callbacks?.onGameLog?.(ev.payload[1]);
      }),
      listen<CrashAnalysisReport>("game-crashed", (ev) => {
        setLaunchingInstanceId(null);
        setRunningInstanceId(null);
        setDownloadProgress(null);
        setActiveCrashReport(ev.payload);
        callbacks?.onGameCrash?.(ev.payload);
      }),
      listen<string>("game-exited-cleanly", () => {
        setLaunchingInstanceId(null);
        setRunningInstanceId(null);
        setDownloadProgress(null);
      }),
    ];

    return () => {
      unlistenPromises.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, [callbacks]);

  return {
    authStore,
    setAuthStore,
    instances,
    setInstances,
    globalSettings,
    setGlobalSettings,
    themes,
    plugins,
    setPlugins,
    javaRuntimes,
    setJavaRuntimes,
    telemetry,
    downloadProgress,
    setDownloadProgress,
    launchingInstanceId,
    setLaunchingInstanceId,
    runningInstanceId,
    setRunningInstanceId,
    activeCrashReport,
    setActiveCrashReport,
    applyThemeToDom,
    refreshAll,
  };
}
