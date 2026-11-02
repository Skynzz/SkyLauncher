import { invoke } from "@tauri-apps/api/core";
import {
  Camera,
  FolderOpen,
  Globe2,
  History,
  Package,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Share2,
  ShieldCheck,
  Trash2,
  UserCheck,
  Wrench,
  Zap,
  X,
} from "lucide-react";
import React, { useCallback, useEffect, useState } from "react";
import type {
  InstanceConfig,
  ModMetadata,
  ModUpdateInfo,
  PackResourceEntry,
  ScreenshotEntry,
  WorldBackupEntry,
  WorldSaveEntry,
} from "../hooks/useLauncher";
import { Language, translations } from "../i18n/translations";

interface SecurityFinding {
  severity: string;
  fileName: string;
  reasonTr: string;
  reasonEn: string;
}

interface SecurityScanReport {
  scannedFiles: number;
  safeFiles: number;
  passed: boolean;
  threats: SecurityFinding[];
}

interface ModManagerModalProps {
  instance: InstanceConfig;
  onClose: () => void;
  onOpenStoreForInstance: (inst: InstanceConfig) => void;
  onQuickPlayWorld: (inst: InstanceConfig, worldFolder: string) => void;
  language: Language;
}

export const ModManagerModal: React.FC<ModManagerModalProps> = ({
  instance,
  onClose,
  onOpenStoreForInstance,
  onQuickPlayWorld,
  language,
}) => {
  const t = translations[language];
  const [activeTab, setActiveTab] = useState<
    | "mods"
    | "resourcepacks"
    | "shaderpacks"
    | "config"
    | "worlds"
    | "screenshots"
  >("mods");
  const [mods, setMods] = useState<ModMetadata[]>([]);
  const [resources, setResources] = useState<PackResourceEntry[]>([]);
  const [worlds, setWorlds] = useState<WorldSaveEntry[]>([]);
  const [backups, setBackups] = useState<WorldBackupEntry[]>([]);
  const [screenshots, setScreenshots] = useState<ScreenshotEntry[]>([]);
  const [modUpdates, setModUpdates] = useState<ModUpdateInfo[]>([]);

  const [loading, setLoading] = useState(false);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [showSkinInput, setShowSkinInput] = useState(false);
  const [skinPathInput, setSkinPathInput] = useState("");

  const loadTabContent = useCallback(async () => {
    setLoading(true);
    try {
      if (activeTab === "mods") {
        const list = await invoke<ModMetadata[]>("get_instance_mods", {
          instanceId: instance.id,
        });
        setMods(list);
      } else if (activeTab === "worlds") {
        const [wList, bList] = await Promise.all([
          invoke<WorldSaveEntry[]>("list_worlds_cmd", {
            instanceId: instance.id,
          }),
          invoke<WorldBackupEntry[]>("list_world_backups_cmd", {
            instanceId: instance.id,
          }),
        ]);
        setWorlds(wList);
        setBackups(bList);
      } else if (activeTab === "screenshots") {
        const shots = await invoke<ScreenshotEntry[]>("list_screenshots_cmd", {
          instanceId: instance.id,
        });
        setScreenshots(shots);
      } else {
        const resList = await invoke<PackResourceEntry[]>(
          "list_instance_resources",
          {
            instanceId: instance.id,
            category: activeTab,
          }
        );
        setResources(resList);
      }
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
  }, [activeTab, instance.id]);

  useEffect(() => {
    loadTabContent();
  }, [loadTabContent]);

  const handleToggleMod = async (fileName: string) => {
    try {
      const updated = await invoke<ModMetadata[]>("toggle_instance_mod", {
        instanceId: instance.id,
        fileName,
      });
      setMods(updated);
    } catch {
      // ignore
    }
  };

  const handleDeleteResource = async (category: string, fileName: string) => {
    try {
      await invoke("delete_instance_resource", {
        instanceId: instance.id,
        category,
        fileName,
      });
      await loadTabContent();
    } catch (err) {
      setFeedback(String(err));
    }
  };

  const handleCheckUpdates = async () => {
    setBusyAction("updates");
    setFeedback(null);
    try {
      const list = await invoke<ModUpdateInfo[]>("check_mod_updates_cmd", {
        instanceId: instance.id,
      });
      setModUpdates(list);
      if (list.length === 0) {
        setFeedback(
          language === "tr"
            ? "Tüm modlar güncel!"
            : "All installed mods are up to date!"
        );
      } else {
        setFeedback(
          language === "tr"
            ? `${list.length} mod için yeni güncelleme bulundu.`
            : `Found updates for ${list.length} mod(s).`
        );
      }
    } catch (err) {
      setFeedback(String(err));
    } finally {
      setBusyAction(null);
    }
  };

  const handleApplyUpdates = async () => {
    setBusyAction("apply-updates");
    try {
      const count = await invoke<number>("apply_mod_updates_cmd", {
        instanceId: instance.id,
        updates: modUpdates,
      });
      setModUpdates([]);
      await loadTabContent();
      setFeedback(
        language === "tr"
          ? `${count} mod başarıyla güncellendi!`
          : `Updated ${count} mod(s) successfully!`
      );
    } catch (err) {
      setFeedback(String(err));
    } finally {
      setBusyAction(null);
    }
  };

  const handleHealDependencies = async () => {
    setBusyAction("heal-deps");
    setFeedback(null);
    try {
      const installed = await invoke<string[]>("heal_dependencies_cmd", {
        instanceId: instance.id,
      });
      await loadTabContent();
      if (installed.length === 0) {
        setFeedback(
          language === "tr"
            ? "✅ Tüm modların zorunlu kütüphane ve gereksinimleri eksiksiz!"
            : "✅ All required mod dependencies are already satisfied!"
        );
      } else {
        setFeedback(
          language === "tr"
            ? `✅ ${installed.length} eksik mod gereksinimi otomatik indirildi ve kuruldu: ${installed.join(", ")}`
            : `✅ Auto-downloaded and installed ${installed.length} missing dependencies: ${installed.join(", ")}`
        );
      }
    } catch (err) {
      setFeedback(String(err));
    } finally {
      setBusyAction(null);
    }
  };

  const handleSecurityScan = async () => {
    setBusyAction("security-scan");
    setFeedback(null);
    try {
      const report = await invoke<SecurityScanReport>("scan_security_cmd", {
        instanceId: instance.id,
      });
      if (report.passed && report.threats.length === 0) {
        setFeedback(
          language === "tr"
            ? `🛡️ SkyShield Taraması Tamamlandı: ${report.scannedFiles} mod (.jar) incelendi — Hiçbir zararlı yazılım, Fractureiser veya Zip-Slip tehdidi bulunmadı.`
            : `🛡️ SkyShield Scan Complete: ${report.scannedFiles} mod (.jar) files scanned — No malware or threats detected.`
        );
      } else {
        const details = report.threats
          .map(
            (f) =>
              `[${f.severity.toUpperCase()}] ${f.fileName}: ${
                language === "tr" ? f.reasonTr : f.reasonEn
              }`
          )
          .join(" | ");
        setFeedback(`⚠️ SkyShield Bulguları: ${details}`);
      }
    } catch (err) {
      setFeedback(String(err));
    } finally {
      setBusyAction(null);
    }
  };

  const handleInstallFpsBoost = async () => {
    setBusyAction("fps");
    setFeedback(null);
    try {
      const installed = await invoke<string[]>("install_fps_boost_cmd", {
        instanceId: instance.id,
      });
      await loadTabContent();
      setFeedback(
        language === "tr"
          ? `FPS Optimizasyon paketi kuruldu (${installed.length} mod eklendi).`
          : `FPS Boost suite installed (${installed.length} mods added).`
      );
    } catch (err) {
      setFeedback(String(err));
    } finally {
      setBusyAction(null);
    }
  };

  const handleInstallLanTunnel = async () => {
    setBusyAction("lan");
    setFeedback(null);
    try {
      await invoke<string[]>("install_lan_tunnel_cmd", {
        instanceId: instance.id,
      });
      await loadTabContent();
      setFeedback(
        language === "tr"
          ? "e4mc (Arkadaşla Oyna) kuruldu! Oyun içinde 'Yerel Ağda Paylaş (Open to LAN)' dediğinizde sohbet ekranında genel sunucu adresiniz belirecek."
          : "e4mc installed! Click 'Open to LAN' in-game to get a public link for friends."
      );
    } catch (err) {
      setFeedback(String(err));
    } finally {
      setBusyAction(null);
    }
  };

  const handleSyncOptions = async () => {
    setBusyAction("sync");
    setFeedback(null);
    try {
      const count = await invoke<number>("sync_options_txt_cmd", {
        sourceInstanceId: instance.id,
      });
      setFeedback(
        language === "tr"
          ? `Bu paketin oyun içi ayarları (options.txt) diğer ${count} pakete kopyalandı.`
          : `Synced options.txt to ${count} other pack(s).`
      );
    } catch (err) {
      setFeedback(String(err));
    } finally {
      setBusyAction(null);
    }
  };

  const handleApplyOfflineSkin = async (e: React.FormEvent) => {
    e.preventDefault();
    const cleaned = skinPathInput.trim().replace(/^"|"$/g, "");
    if (!cleaned) return;
    setBusyAction("skin");
    setFeedback(null);
    try {
      await invoke<string>("apply_offline_skin_cmd", {
        instanceId: instance.id,
        skinPath: cleaned,
      });
      setShowSkinInput(false);
      await loadTabContent();
      setFeedback(
        language === "tr"
          ? "Özel Skin (.png) ve CustomSkinLoader bu pakete başarıyla uygulandı!"
          : "Custom Skin (.png) and CustomSkinLoader applied to this pack!"
      );
    } catch (err) {
      setFeedback(String(err));
    } finally {
      setBusyAction(null);
    }
  };

  const handleBackupWorld = async (folderName: string) => {
    setBusyAction("backup-" + folderName);
    try {
      await invoke<WorldBackupEntry>("create_world_backup_cmd", {
        instanceId: instance.id,
        folderName,
      });
      await loadTabContent();
      setFeedback(
        language === "tr"
          ? `'${folderName}' dünyası yedeklendi.`
          : `Backed up world '${folderName}'.`
      );
    } catch (err) {
      setFeedback(String(err));
    } finally {
      setBusyAction(null);
    }
  };

  const handleRestoreBackup = async (backupFileName: string) => {
    setBusyAction("restore-" + backupFileName);
    try {
      const worldName = await invoke<string>("restore_world_backup_cmd", {
        instanceId: instance.id,
        backupFileName,
      });
      await loadTabContent();
      setFeedback(
        language === "tr"
          ? `'${worldName}' dünyası yedekten geri yüklendi!`
          : `Restored world '${worldName}' from backup!`
      );
    } catch (err) {
      setFeedback(String(err));
    } finally {
      setBusyAction(null);
    }
  };

  return (
    <div className="fixed inset-0 z-50 bg-black/65 backdrop-blur-sm flex items-center justify-center p-4">
      <div className="w-full max-w-3xl rounded-2xl bg-bg-card border border-border-subtle shadow-2xl flex flex-col h-[85vh] max-h-[720px] overflow-hidden">
        {/* Header */}
        <div className="p-4 border-b border-border-subtle flex items-center justify-between gap-2 shrink-0">
          <div className="flex items-center gap-2 min-w-0">
            <Package className="w-4 h-4 text-accent-secondary shrink-0" />
            <h2 className="text-sm font-bold text-text-primary truncate">
              {instance.name} ({instance.mcVersion} • {instance.modloader}) —{" "}
              {t.modManagerTitle}
            </h2>
          </div>
          <div className="flex items-center gap-1.5 shrink-0">
            <button
              onClick={() => onOpenStoreForInstance(instance)}
              className="px-3 py-1.5 rounded-xl bg-accent-primary hover:opacity-90 text-white text-xs font-semibold flex items-center gap-1 transition"
            >
              <Plus className="w-3.5 h-3.5" />
              <span>{t.addContentFromStoreBtn}</span>
            </button>
            <button
              onClick={loadTabContent}
              className="p-1.5 rounded-lg bg-bg-elevated hover:bg-bg-secondary text-text-secondary"
            >
              <RefreshCw
                className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`}
              />
            </button>
            <button
              onClick={onClose}
              className="p-1.5 rounded-lg bg-bg-elevated hover:bg-bg-secondary text-text-secondary"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        {/* One-Click Smart Tools Bar */}
        <div className="px-4 py-2 bg-bg-secondary border-b border-border-subtle flex flex-wrap items-center gap-1.5 shrink-0">
          <button
            disabled={busyAction !== null}
            onClick={handleHealDependencies}
            className="px-2.5 py-1.5 rounded-xl bg-accent-primary/15 hover:bg-accent-primary/25 border border-accent-primary/35 text-[11px] font-semibold text-accent-secondary flex items-center gap-1.5 transition"
          >
            <Wrench
              className={`w-3 h-3 ${busyAction === "heal-deps" ? "animate-spin" : ""}`}
            />
            <span>
              {language === "tr"
                ? "Eksik Gereksinimleri Tamamla"
                : "Auto-Heal Dependencies"}
            </span>
          </button>

          <button
            disabled={busyAction !== null}
            onClick={handleSecurityScan}
            className="px-2.5 py-1.5 rounded-xl bg-emerald-500/10 hover:bg-emerald-500/20 border border-emerald-500/30 text-[11px] font-semibold text-emerald-300 flex items-center gap-1.5 transition"
          >
            <ShieldCheck className="w-3 h-3" />
            <span>
              {language === "tr" ? "SkyShield Güvenlik Taraması" : "SkyShield Scan"}
            </span>
          </button>

          <button
            disabled={busyAction !== null}
            onClick={handleCheckUpdates}
            className="px-2.5 py-1.5 rounded-xl bg-bg-card hover:bg-bg-elevated border border-border-subtle text-[11px] font-medium text-text-primary flex items-center gap-1.5 transition"
          >
            <RefreshCw className="w-3 h-3 text-accent-secondary" />
            <span>{t.checkUpdatesBtn}</span>
          </button>

          {modUpdates.length > 0 && (
            <button
              disabled={busyAction !== null}
              onClick={handleApplyUpdates}
              className="px-2.5 py-1.5 rounded-xl bg-emerald-600 hover:bg-emerald-500 text-white text-[11px] font-semibold flex items-center gap-1 transition"
            >
              <span>
                {t.applyUpdatesBtn} ({modUpdates.length})
              </span>
            </button>
          )}

          <button
            disabled={busyAction !== null}
            onClick={handleInstallFpsBoost}
            className="px-2.5 py-1.5 rounded-xl bg-bg-card hover:bg-bg-elevated border border-border-subtle text-[11px] font-medium text-emerald-300 flex items-center gap-1.5 transition"
          >
            <Zap className="w-3 h-3" />
            <span>{t.fpsBoostBtn}</span>
          </button>

          <button
            disabled={busyAction !== null}
            onClick={handleInstallLanTunnel}
            className="px-2.5 py-1.5 rounded-xl bg-bg-card hover:bg-bg-elevated border border-border-subtle text-[11px] font-medium text-sky-300 flex items-center gap-1.5 transition"
          >
            <Globe2 className="w-3 h-3" />
            <span>{t.lanTunnelBtn}</span>
          </button>

          <button
            disabled={busyAction !== null}
            onClick={() => setShowSkinInput(!showSkinInput)}
            className="px-2.5 py-1.5 rounded-xl bg-bg-card hover:bg-bg-elevated border border-border-subtle text-[11px] font-medium text-amber-300 flex items-center gap-1.5 transition"
          >
            <UserCheck className="w-3 h-3" />
            <span>{t.offlineSkinBtn}</span>
          </button>

          <button
            disabled={busyAction !== null}
            onClick={handleSyncOptions}
            className="px-2.5 py-1.5 rounded-xl bg-bg-card hover:bg-bg-elevated border border-border-subtle text-[11px] font-medium text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition"
          >
            <Share2 className="w-3 h-3" />
            <span>{t.syncOptionsBtn}</span>
          </button>
        </div>

        {showSkinInput && (
          <form
            onSubmit={handleApplyOfflineSkin}
            className="px-4 py-2.5 bg-bg-primary border-b border-border-subtle flex items-center gap-2"
          >
            <input
              type="text"
              value={skinPathInput}
              onChange={(e) => setSkinPathInput(e.target.value)}
              placeholder="C:\Users\...\skin.png dosya yolunu yapıştırın..."
              className="flex-1 px-3 py-1.5 rounded-lg bg-bg-card border border-border-subtle text-xs text-text-primary"
            />
            <button
              type="submit"
              className="px-3 py-1.5 rounded-lg bg-accent-primary text-white text-xs font-semibold"
            >
              Uygula
            </button>
          </form>
        )}

        {feedback && (
          <div className="mx-4 mt-3 p-2.5 rounded-xl bg-accent-primary/15 border border-accent-primary/30 text-xs text-text-primary flex items-center justify-between">
            <span>{feedback}</span>
            <button
              onClick={() => setFeedback(null)}
              className="text-text-secondary hover:text-white"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        )}

        {/* Tabs */}
        <div className="px-4 pt-1.5 border-b border-border-subtle flex items-center gap-1 overflow-x-auto shrink-0">
          {(
            [
              { id: "mods", label: `${t.tabMods} (${mods.length})` },
              { id: "resourcepacks", label: t.tabResourcePacks },
              { id: "shaderpacks", label: t.tabShaderPacks },
              { id: "config", label: t.tabConfigs },
              { id: "worlds", label: t.tabWorlds },
              { id: "screenshots", label: t.tabScreenshots },
            ] as const
          ).map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`px-3.5 py-2.5 text-xs font-semibold border-b-2 whitespace-nowrap transition inline-flex items-center ${
                activeTab === tab.id
                  ? "border-accent-primary text-text-primary"
                  : "border-transparent text-text-secondary hover:text-text-primary"
              }`}
            >
              {tab.label}
            </button>
          ))}
        </div>

        {/* Tab Content */}
        <div className="p-4 overflow-y-auto flex-1 min-h-0 space-y-2">
          {activeTab === "mods" ? (
            mods.length === 0 ? (
              <div className="py-10 text-center text-xs text-text-muted space-y-2">
                <FolderOpen className="w-7 h-7 mx-auto text-text-muted/60" />
                <p>{t.noModsFound}</p>
              </div>
            ) : (
              mods.map((mod) => (
                <div
                  key={mod.fileName}
                  className={`p-3 rounded-xl border flex items-center justify-between gap-3 transition ${
                    mod.enabled
                      ? "bg-bg-secondary border-border-subtle"
                      : "bg-bg-primary/50 border-border-subtle/40 opacity-60"
                  }`}
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="font-semibold text-xs text-text-primary truncate">
                        {mod.name}
                      </span>
                      <span className="text-[11px] text-text-muted">
                        v{mod.version}
                      </span>
                    </div>
                    {mod.description && (
                      <p className="text-[11px] text-text-secondary line-clamp-1 mt-0.5">
                        {mod.description}
                      </p>
                    )}
                  </div>

                  <div className="flex items-center gap-2 shrink-0">
                    <button
                      onClick={() => handleToggleMod(mod.fileName)}
                      className={`px-3 py-1 rounded-lg text-xs font-medium transition ${
                        mod.enabled
                          ? "bg-accent-primary/15 text-accent-secondary"
                          : "bg-bg-elevated text-text-muted"
                      }`}
                    >
                      {mod.enabled ? t.enabledState : t.disabledState}
                    </button>

                    <button
                      onClick={() => handleDeleteResource("mods", mod.fileName)}
                      title="Sil"
                      className="p-1.5 rounded-lg bg-red-500/10 hover:bg-red-500/20 text-red-400 transition"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              ))
            )
          ) : activeTab === "worlds" ? (
            <div className="space-y-4">
              <div className="space-y-2">
                <h3 className="text-xs font-bold text-text-primary flex items-center gap-1.5">
                  <Save className="w-3.5 h-3.5 text-accent-secondary" />
                  <span>Kayıtlı Dünyalar (saves/)</span>
                </h3>
                {worlds.length === 0 ? (
                  <p className="text-xs text-text-muted py-3">
                    Henüz kayıtlı dünya bulunmuyor.
                  </p>
                ) : (
                  worlds.map((w) => (
                    <div
                      key={w.folderName}
                      className="p-3 rounded-xl bg-bg-secondary border border-border-subtle flex items-center justify-between gap-3"
                    >
                      <div>
                        <div className="text-xs font-semibold text-text-primary">
                          {w.displayName}
                        </div>
                        <div className="text-[11px] text-text-muted">
                          {(w.sizeBytes / (1024 * 1024)).toFixed(1)} MB
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        <button
                          onClick={() => handleBackupWorld(w.folderName)}
                          className="px-3 py-1.5 rounded-xl bg-bg-elevated hover:bg-bg-card text-xs font-medium text-text-primary flex items-center gap-1"
                        >
                          <History className="w-3.5 h-3.5 text-amber-400" />
                          <span>{t.backupWorldBtn}</span>
                        </button>
                        <button
                          onClick={() => {
                            onQuickPlayWorld(instance, w.folderName);
                            onClose();
                          }}
                          className="px-3 py-1.5 rounded-xl bg-accent-primary hover:opacity-90 text-white text-xs font-semibold flex items-center gap-1"
                        >
                          <Play className="w-3 h-3 fill-current" />
                          <span>{t.quickPlayWorldBtn}</span>
                        </button>
                      </div>
                    </div>
                  ))
                )}
              </div>

              <div className="space-y-2 pt-2 border-t border-border-subtle">
                <h3 className="text-xs font-bold text-text-primary flex items-center gap-1.5">
                  <History className="w-3.5 h-3.5 text-emerald-400" />
                  <span>Dünya Yedekleri / Zaman Makinesi (backups/)</span>
                </h3>
                {backups.length === 0 ? (
                  <p className="text-xs text-text-muted py-2">
                    Henüz alınmış dünya yedeği yok.
                  </p>
                ) : (
                  backups.map((b) => (
                    <div
                      key={b.fileName}
                      className="p-2.5 rounded-xl bg-bg-secondary border border-border-subtle flex items-center justify-between text-xs"
                    >
                      <div>
                        <span className="font-medium text-text-primary">
                          {b.fileName}
                        </span>
                        <span className="text-text-muted text-[11px] ml-2">
                          ({(b.sizeBytes / (1024 * 1024)).toFixed(1)} MB)
                        </span>
                      </div>
                      <button
                        onClick={() => handleRestoreBackup(b.fileName)}
                        className="px-3 py-1 rounded-lg bg-emerald-500/20 hover:bg-emerald-500/30 text-emerald-300 font-medium flex items-center gap-1"
                      >
                        <RotateCcw className="w-3 h-3" />
                        <span>{t.restoreBackupBtn}</span>
                      </button>
                    </div>
                  ))
                )}
              </div>
            </div>
          ) : activeTab === "screenshots" ? (
            screenshots.length === 0 ? (
              <div className="py-10 text-center text-xs text-text-muted space-y-2">
                <Camera className="w-7 h-7 mx-auto text-text-muted/60" />
                <p>Bu pakette henüz ekran görüntüsü (F2) alınmamış.</p>
              </div>
            ) : (
              <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
                {screenshots.map((shot) => (
                  <div
                    key={shot.fileName}
                    className="rounded-xl overflow-hidden bg-bg-secondary border border-border-subtle"
                  >
                    <img
                      src={shot.dataUrl}
                      alt={shot.fileName}
                      className="w-full h-32 object-cover"
                    />
                    <div className="p-2 text-[11px] text-text-secondary truncate">
                      {shot.fileName}
                    </div>
                  </div>
                ))}
              </div>
            )
          ) : resources.length === 0 ? (
            <div className="py-10 text-center text-xs text-text-muted">—</div>
          ) : (
            resources.map((res) => (
              <div
                key={res.fileName}
                className="p-2.5 rounded-xl bg-bg-secondary border border-border-subtle flex items-center justify-between text-xs"
              >
                <span className="text-text-primary truncate">
                  {res.fileName}
                </span>
                <div className="flex items-center gap-2.5 shrink-0">
                  <span className="text-text-muted text-[11px]">
                    {(res.sizeBytes / 1024).toFixed(0)} KB
                  </span>
                  <button
                    onClick={() =>
                      handleDeleteResource(activeTab, res.fileName)
                    }
                    className="p-1.5 rounded-lg bg-red-500/10 hover:bg-red-500/20 text-red-400 transition"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
};
