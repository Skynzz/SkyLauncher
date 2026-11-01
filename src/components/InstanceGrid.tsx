import { invoke } from "@tauri-apps/api/core";
import {
  Check,
  Clock,
  Copy,
  CopyPlus,
  Download,
  FolderSync,
  ImagePlus,
  Package,
  Play,
  Plus,
  Search,
  Share2,
  Sliders,
  Square,
  Trash2,
  X,
} from "lucide-react";
import React, { useState } from "react";
import type {
  CrashAnalysisReport,
  DetectedJavaRuntime,
  DownloadProgressEvent,
  EffectiveLaunchSettings,
  GlobalSettings,
  InstanceConfig,
} from "../hooks/useLauncher";
import { Language, translations } from "../i18n/translations";

interface InstanceGridProps {
  instances: InstanceConfig[];
  setInstances: React.Dispatch<React.SetStateAction<InstanceConfig[]>>;
  globalSettings: GlobalSettings;
  javaRuntimes: DetectedJavaRuntime[];
  launchingInstanceId: string | null;
  setLaunchingInstanceId: (id: string | null) => void;
  runningInstanceId: string | null;
  setRunningInstanceId: (id: string | null) => void;
  downloadProgress: DownloadProgressEvent | null;
  setDownloadProgress: (prog: DownloadProgressEvent | null) => void;
  onOpenModManager: (inst: InstanceConfig) => void;
  onOpenShareModal: (inst: InstanceConfig | null) => void;
  onNavigateDownloader: () => void;
  onTriggerCrashReport: (report: CrashAnalysisReport) => void;
  onPreLaunchHook: (inst: InstanceConfig) => void;
  language: Language;
}

export const InstanceGrid: React.FC<InstanceGridProps> = ({
  instances,
  setInstances,
  globalSettings,
  javaRuntimes,
  launchingInstanceId,
  setLaunchingInstanceId,
  runningInstanceId,
  setRunningInstanceId,
  downloadProgress,
  setDownloadProgress,
  onOpenModManager,
  onOpenShareModal,
  onNavigateDownloader,
  onPreLaunchHook,
  language,
}) => {
  const t = translations[language];
  const [searchQuery, setSearchQuery] = useState("");
  const [editingInstance, setEditingInstance] = useState<InstanceConfig | null>(
    null
  );
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [launchError, setLaunchError] = useState<string | null>(null);
  const [copiedError, setCopiedError] = useState(false);

  const filteredInstances = instances.filter(
    (i) =>
      i.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      i.mcVersion.toLowerCase().includes(searchQuery.toLowerCase()) ||
      i.modloader.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const handleLaunch = async (inst: InstanceConfig) => {
    setLaunchError(null);
    setLaunchingInstanceId(inst.id);
    onPreLaunchHook(inst);
    try {
      await invoke<EffectiveLaunchSettings>("launch_instance", {
        instanceId: inst.id,
        quickPlayWorld: null,
      });
    } catch (err) {
      setLaunchingInstanceId(null);
      setDownloadProgress(null);
      const msg = String(err);
      if (!msg.toLowerCase().includes("iptal")) {
        setLaunchError(msg);
      }
    }
  };

  const handleCancelDownload = async (instanceId?: string) => {
    try {
      await invoke("cancel_launch", {
        instanceId: instanceId || launchingInstanceId || null,
      });
    } catch {
      // ignore
    }
    setLaunchingInstanceId(null);
    setDownloadProgress(null);
  };

  const handleStopGame = async (instanceId: string) => {
    try {
      await invoke("stop_instance", { instanceId });
    } catch (err) {
      setLaunchError(String(err));
    }
    setRunningInstanceId(null);
    setLaunchingInstanceId(null);
  };

  const handleCopyLaunchError = async () => {
    if (!launchError) return;
    try {
      await navigator.clipboard.writeText(launchError);
      setCopiedError(true);
      setTimeout(() => setCopiedError(false), 2000);
    } catch {
      // ignore
    }
  };

  const handleSaveJvmSettings = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!editingInstance) return;
    try {
      const saved = await invoke<InstanceConfig>("update_instance", {
        instance: editingInstance,
      });
      setInstances((prev) =>
        prev.map((item) => (item.id === saved.id ? saved : item))
      );
      setEditingInstance(null);
    } catch (err) {
      setLaunchError(String(err));
    }
  };

  const handleDeleteInstance = async (instanceId: string) => {
    try {
      const remaining = await invoke<InstanceConfig[]>("delete_instance", {
        instanceId,
        deleteFiles: true,
      });
      setInstances(remaining);
      setEditingInstance(null);
      setConfirmDeleteId(null);
    } catch (err) {
      setLaunchError(String(err));
    }
  };

  const handleCloneInstance = async (instanceId: string) => {
    try {
      const cloned = await invoke<InstanceConfig>("clone_instance_cmd", {
        instanceId,
        newName: null,
      });
      setInstances((prev) => [...prev, cloned]);
    } catch (err) {
      setLaunchError(String(err));
    }
  };

  const handleRescanCurseForge = async () => {
    try {
      const updated = await invoke<InstanceConfig[]>(
        "rescan_curseforge_instances",
        { customDir: null }
      );
      setInstances(updated);
    } catch (err) {
      setLaunchError(String(err));
    }
  };

  const progressPercent = downloadProgress
    ? Math.min(
        100,
        Math.round(
          (downloadProgress.completedFiles /
            Math.max(1, downloadProgress.totalFiles)) *
            100
        )
      )
    : 0;

  return (
    <div className="flex-1 h-screen overflow-y-auto p-6 flex flex-col justify-between">
      <div className="space-y-6">
        {/* Clean Header Bar */}
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="relative flex-1 max-w-sm">
            <Search className="w-4 h-4 text-text-muted absolute left-3.5 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={t.searchInstances}
              className="w-full pl-10 pr-4 py-2 rounded-xl bg-bg-card border border-border-subtle text-sm text-text-primary focus:outline-none focus:border-accent-primary"
            />
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={() => onOpenShareModal(null)}
              className="flex items-center gap-1.5 px-3.5 py-2 rounded-xl bg-bg-card hover:bg-bg-elevated border border-border-subtle text-xs font-medium text-text-secondary hover:text-text-primary transition"
            >
              <Download className="w-3.5 h-3.5 text-accent-secondary" />
              <span>{t.importOrShareBtn}</span>
            </button>

            {globalSettings.curseforgeInstancesDir && (
              <button
                onClick={handleRescanCurseForge}
                className="flex items-center gap-2 px-3.5 py-2 rounded-xl bg-bg-card hover:bg-bg-elevated border border-border-subtle text-xs font-medium text-text-secondary hover:text-text-primary transition"
              >
                <FolderSync className="w-4 h-4" />
                <span>{t.rescanCurseForge}</span>
              </button>
            )}

            <button
              onClick={onNavigateDownloader}
              className="flex items-center gap-1.5 px-4 py-2 rounded-xl bg-accent-primary hover:opacity-90 text-white text-xs font-semibold transition"
            >
              <Plus className="w-4 h-4" />
              <span>{t.newCustomInstance}</span>
            </button>
          </div>
        </div>

        {launchError && (
          <div className="p-3.5 rounded-xl bg-red-500/10 border border-red-500/25 text-xs text-red-300 flex items-center justify-between gap-3">
            <span className="break-all flex-1">{launchError}</span>
            <div className="flex items-center gap-2 shrink-0">
              <button
                onClick={handleCopyLaunchError}
                className="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-red-500/20 hover:bg-red-500/30 text-red-100 text-[11px] font-medium transition"
              >
                {copiedError ? (
                  <>
                    <Check className="w-3 h-3" />
                    <span>{t.copiedBtn}</span>
                  </>
                ) : (
                  <>
                    <Copy className="w-3 h-3" />
                    <span>{t.copyBtn}</span>
                  </>
                )}
              </button>
              <button
                onClick={() => setLaunchError(null)}
                className="p-1 text-red-200 hover:text-white font-semibold"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
          </div>
        )}

        {/* Clean Instance Cards */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filteredInstances.map((inst) => {
            const isLaunching = launchingInstanceId === inst.id;
            const isRunning = runningInstanceId === inst.id;
            const effectiveRam = inst.overrideMemory
              ? inst.maxRamMb
              : globalSettings.defaultMaxRamMb;
            const playtimeMins = inst.playtimeMinutes || 0;

            return (
              <div
                key={inst.id}
                className="rounded-2xl bg-bg-card border border-border-subtle p-4 flex flex-col justify-between space-y-4 hover:border-accent-primary/40 transition"
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="flex items-center gap-3 min-w-0">
                    {inst.iconDataUrl ? (
                      <img
                        src={inst.iconDataUrl}
                        alt={inst.name}
                        className="w-11 h-11 rounded-xl object-cover bg-bg-elevated border border-border-subtle shrink-0"
                      />
                    ) : (
                      <div className="w-11 h-11 rounded-xl bg-bg-elevated border border-border-subtle flex items-center justify-center text-accent-secondary font-bold text-base shrink-0">
                        {inst.name.slice(0, 2).toUpperCase()}
                      </div>
                    )}
                    <div className="min-w-0">
                      <h3 className="font-semibold text-sm text-text-primary truncate">
                        {inst.name}
                      </h3>
                      <div className="text-xs text-text-secondary mt-0.5 flex flex-wrap items-center gap-1.5">
                        <span>{inst.mcVersion}</span>
                        <span>•</span>
                        <span className="capitalize">{inst.modloader}</span>
                        <span>•</span>
                        <span>{(effectiveRam / 1024).toFixed(1)} GB</span>
                        {playtimeMins > 0 && (
                          <>
                            <span>•</span>
                            <span className="inline-flex items-center gap-0.5 text-text-muted">
                              <Clock className="w-3 h-3" />
                              {playtimeMins >= 60
                                ? `${(playtimeMins / 60).toFixed(1)} sa`
                                : `${playtimeMins} dk`}
                            </span>
                          </>
                        )}
                      </div>
                    </div>
                  </div>

                  {/* Quick Card Actions: Share, Clone, Direct Delete */}
                  <div className="flex items-center gap-1 shrink-0">
                    <button
                      onClick={() => onOpenShareModal(inst)}
                      title={t.shareBtn}
                      className="p-1.5 rounded-lg bg-bg-elevated hover:bg-bg-secondary text-text-secondary hover:text-text-primary transition"
                    >
                      <Share2 className="w-3.5 h-3.5" />
                    </button>

                    <button
                      onClick={() => handleCloneInstance(inst.id)}
                      title={t.cloneBtn}
                      className="p-1.5 rounded-lg bg-bg-elevated hover:bg-bg-secondary text-text-secondary hover:text-text-primary transition"
                    >
                      <CopyPlus className="w-3.5 h-3.5" />
                    </button>

                    {confirmDeleteId === inst.id ? (
                      <div className="flex items-center gap-1">
                        <button
                          onClick={() => handleDeleteInstance(inst.id)}
                          className="px-2 py-1 rounded-lg bg-red-500 text-white text-[10px] font-semibold"
                        >
                          Sil
                        </button>
                        <button
                          onClick={() => setConfirmDeleteId(null)}
                          className="p-1 rounded-lg bg-bg-elevated text-text-secondary text-[10px]"
                        >
                          ✕
                        </button>
                      </div>
                    ) : (
                      <button
                        onClick={() => setConfirmDeleteId(inst.id)}
                        title={t.deleteInstance}
                        className="p-1.5 rounded-lg bg-bg-elevated hover:bg-red-500/20 text-text-secondary hover:text-red-400 transition"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    )}
                  </div>
                </div>

                {/* Action Bar */}
                <div className="flex items-center gap-2">
                  {isRunning ? (
                    <button
                      onClick={() => handleStopGame(inst.id)}
                      className="flex-1 flex items-center justify-center gap-2 py-2 px-4 rounded-xl bg-red-500 hover:bg-red-600 text-white font-semibold text-xs transition"
                    >
                      <Square className="w-3.5 h-3.5 fill-current" />
                      <span>{t.stopBtn}</span>
                    </button>
                  ) : isLaunching ? (
                    <div className="flex-1 flex items-center gap-1.5">
                      <button
                        disabled
                        className="flex-1 flex items-center justify-center gap-2 py-2 px-3 rounded-xl bg-accent-primary/60 text-white font-semibold text-xs cursor-wait"
                      >
                        <span>{t.launchingBtn}</span>
                      </button>
                      <button
                        onClick={() => handleCancelDownload(inst.id)}
                        title={t.cancelBtn}
                        className="px-2.5 py-2 rounded-xl bg-red-500/20 hover:bg-red-500/30 text-red-300 text-xs font-medium transition"
                      >
                        {t.cancelBtn}
                      </button>
                    </div>
                  ) : (
                    <button
                      onClick={() => handleLaunch(inst)}
                      className="flex-1 flex items-center justify-center gap-2 py-2 px-4 rounded-xl bg-accent-primary hover:opacity-90 text-white font-semibold text-xs transition"
                    >
                      <Play className="w-3.5 h-3.5 fill-current" />
                      <span>{t.launchBtn}</span>
                    </button>
                  )}

                  <button
                    onClick={() => onOpenModManager(inst)}
                    className="px-3 py-2 rounded-xl bg-bg-elevated hover:bg-bg-secondary text-xs font-medium text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition"
                  >
                    <Package className="w-3.5 h-3.5" />
                    <span>{t.modsBtn}</span>
                  </button>

                  <button
                    onClick={() => setEditingInstance({ ...inst })}
                    className="p-2 rounded-xl bg-bg-elevated hover:bg-bg-secondary text-text-secondary hover:text-text-primary transition"
                    title={t.jvmSettingsBtn}
                  >
                    <Sliders className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Bottom Clean Download Progress Bar with Stage Indicator & Cancel Button */}
      {downloadProgress && (
        <div className="mt-4 p-4 rounded-xl bg-bg-card border border-border-subtle space-y-2.5">
          <div className="flex items-center justify-between gap-3 text-xs">
            <div className="min-w-0 flex-1">
              <div className="font-semibold text-text-primary truncate">
                {downloadProgress.stageText || t.downloadingStatus}
              </div>
              <div className="text-[11px] text-text-secondary truncate mt-0.5">
                {downloadProgress.currentLabel} (
                {downloadProgress.completedFiles}/{downloadProgress.totalFiles})
              </div>
            </div>
            <div className="flex items-center gap-2.5 shrink-0">
              <span className="text-text-primary font-semibold">
                %{progressPercent}
              </span>
              <button
                onClick={() =>
                  handleCancelDownload(downloadProgress.instanceId)
                }
                className="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-red-500/15 hover:bg-red-500/25 text-red-300 text-xs font-medium transition"
              >
                <X className="w-3.5 h-3.5" />
                <span>{t.cancelBtn}</span>
              </button>
            </div>
          </div>
          <div className="w-full h-1.5 rounded-full bg-bg-elevated overflow-hidden">
            <div
              className="h-full bg-accent-primary transition-all duration-150"
              style={{ width: `${progressPercent}%` }}
            />
          </div>
        </div>
      )}

      {/* Simplified Pack Settings Modal */}
      {editingInstance && (
        <div className="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm flex items-center justify-center p-4">
          <form
            onSubmit={handleSaveJvmSettings}
            className="w-full max-w-md rounded-2xl bg-bg-card border border-border-subtle p-5 space-y-4 shadow-xl"
          >
            <div className="flex items-center justify-between border-b border-border-subtle pb-3">
              <div>
                <h2 className="text-sm font-bold text-text-primary">
                  {t.jvmModalTitle}
                </h2>
                <p className="text-xs text-text-secondary">
                  {editingInstance.name}
                </p>
              </div>
              <button
                type="button"
                onClick={() => handleDeleteInstance(editingInstance.id)}
                className="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-red-500/10 hover:bg-red-500/20 text-red-400 text-xs font-medium"
              >
                <Trash2 className="w-3.5 h-3.5" />
                <span>{t.deleteInstance}</span>
              </button>
            </div>

            {/* Custom Logo & Pack Name Editor */}
            <div className="flex items-center gap-3 p-3 rounded-xl bg-bg-secondary border border-border-subtle">
              <label className="relative w-14 h-14 rounded-xl border border-dashed border-border-subtle bg-bg-card flex flex-col items-center justify-center cursor-pointer overflow-hidden shrink-0 hover:border-accent-primary">
                <input
                  type="file"
                  accept="image/png,image/jpeg,image/webp,image/gif"
                  className="hidden"
                  onChange={(e) => {
                    const file = e.target.files?.[0];
                    if (!file) return;
                    const reader = new FileReader();
                    reader.onload = () => {
                      if (typeof reader.result === "string") {
                        setEditingInstance({
                          ...editingInstance,
                          iconDataUrl: reader.result,
                        });
                      }
                    };
                    reader.readAsDataURL(file);
                  }}
                />
                {editingInstance.iconDataUrl ? (
                  <img
                    src={editingInstance.iconDataUrl}
                    alt="Logo"
                    className="w-full h-full object-cover"
                  />
                ) : (
                  <>
                    <ImagePlus className="w-4 h-4 text-accent-secondary" />
                    <span className="text-[9px] text-text-muted mt-0.5">
                      Logo
                    </span>
                  </>
                )}
              </label>

              <div className="flex-1 space-y-1">
                <label className="block text-[11px] text-text-secondary">
                  {t.instanceNameLabel}
                </label>
                <input
                  type="text"
                  value={editingInstance.name}
                  onChange={(e) =>
                    setEditingInstance({
                      ...editingInstance,
                      name: e.target.value,
                    })
                  }
                  className="w-full px-2.5 py-1.5 rounded-lg bg-bg-card border border-border-subtle text-xs text-text-primary"
                />
                {editingInstance.iconDataUrl && (
                  <button
                    type="button"
                    onClick={() =>
                      setEditingInstance({
                        ...editingInstance,
                        iconDataUrl: null,
                      })
                    }
                    className="text-[10px] text-red-400 hover:underline"
                  >
                    {language === "tr" ? "Görsel Logoyu Kaldır" : "Remove Custom Logo"}
                  </button>
                )}
              </div>
            </div>

            <label className="flex items-center gap-2.5 text-xs font-medium text-text-primary cursor-pointer">
              <input
                type="checkbox"
                checked={editingInstance.overrideMemory}
                onChange={(e) =>
                  setEditingInstance({
                    ...editingInstance,
                    overrideMemory: e.target.checked,
                  })
                }
                className="rounded accent-sky-500"
              />
              <span>{t.overrideMemoryLabel}</span>
            </label>

            {editingInstance.overrideMemory && (
              <div className="p-3.5 rounded-xl bg-bg-secondary border border-border-subtle space-y-2">
                <div className="flex justify-between text-xs">
                  <span className="text-text-secondary">{t.maxRamLabel}</span>
                  <span className="font-semibold text-text-primary">
                    {editingInstance.maxRamMb} MB (
                    {(editingInstance.maxRamMb / 1024).toFixed(1)} GB)
                  </span>
                </div>
                <input
                  type="range"
                  min={2048}
                  max={16384}
                  step={512}
                  value={editingInstance.maxRamMb}
                  onChange={(e) =>
                    setEditingInstance({
                      ...editingInstance,
                      maxRamMb: Number(e.target.value),
                    })
                  }
                  className="w-full accent-sky-500"
                />
              </div>
            )}

            <label className="flex items-center gap-2.5 text-xs font-medium text-text-primary cursor-pointer">
              <input
                type="checkbox"
                checked={editingInstance.overrideJava}
                onChange={(e) =>
                  setEditingInstance({
                    ...editingInstance,
                    overrideJava: e.target.checked,
                  })
                }
                className="rounded accent-sky-500"
              />
              <span>{t.overrideJavaLabel}</span>
            </label>

            {editingInstance.overrideJava && (
              <div className="space-y-2">
                <input
                  type="text"
                  value={editingInstance.javaPath || ""}
                  onChange={(e) =>
                    setEditingInstance({
                      ...editingInstance,
                      javaPath: e.target.value || null,
                    })
                  }
                  placeholder="javaw.exe yolu..."
                  className="w-full px-3 py-2 rounded-lg bg-bg-secondary border border-border-subtle text-xs text-text-primary"
                />
                {javaRuntimes.length > 0 && (
                  <div className="flex flex-wrap gap-1.5">
                    {javaRuntimes.map((rt) => (
                      <button
                        type="button"
                        key={rt.path}
                        onClick={() =>
                          setEditingInstance({
                            ...editingInstance,
                            javaPath: rt.path,
                          })
                        }
                        className="px-2 py-1 rounded bg-bg-elevated text-[11px] text-text-secondary hover:text-text-primary"
                      >
                        Java {rt.majorVersion}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )}

            <div className="flex justify-end gap-2 pt-2">
              <button
                type="button"
                onClick={() => setEditingInstance(null)}
                className="px-4 py-2 rounded-xl bg-bg-elevated text-xs font-medium text-text-secondary"
              >
                {t.cancel}
              </button>
              <button
                type="submit"
                className="px-4 py-2 rounded-xl bg-accent-primary text-xs font-semibold text-white"
              >
                {t.saveChanges}
              </button>
            </div>
          </form>
        </div>
      )}
    </div>
  );
};
