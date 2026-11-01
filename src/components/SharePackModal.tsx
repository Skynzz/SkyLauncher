import { invoke } from "@tauri-apps/api/core";
import {
  Archive,
  Check,
  Code2,
  Copy,
  Download,
  FileUp,
  FolderOpen,
  Server,
  Share2,
  Upload,
  X,
} from "lucide-react";
import React, { useRef, useState } from "react";
import type {
  InstanceConfig,
  SkyCodeExportResult,
} from "../hooks/useLauncher";
import { Language, translations } from "../i18n/translations";

interface SharePackModalProps {
  instance: InstanceConfig | null;
  onClose: () => void;
  onPackImported: (created: InstanceConfig) => void;
  language: Language;
}

export const SharePackModal: React.FC<SharePackModalProps> = ({
  instance,
  onClose,
  onPackImported,
  language,
}) => {
  const t = translations[language];
  const [activeTab, setActiveTab] = useState<"export" | "import">(
    instance ? "export" : "import"
  );

  // Export states
  const [exportPath, setExportPath] = useState(
    instance
      ? `C:\\Users\\Public\\Downloads\\${instance.name.replace(
          /[^a-zA-Z0-9_-]/g,
          "_"
        )}`
      : "C:\\Users\\Public\\Downloads\\SkyPack"
  );
  const [skyCodeResult, setSkyCodeResult] =
    useState<SkyCodeExportResult | null>(null);
  const [copiedCode, setCopiedCode] = useState(false);
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  // Import states
  const [importInput, setImportInput] = useState("");
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [isDraggingFile, setIsDraggingFile] = useState(false);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const handleExportFullSkypack = async () => {
    if (!instance) return;
    setBusy(true);
    setErrorMsg(null);
    setFeedback(null);
    try {
      const target = exportPath.endsWith(".skypack")
        ? exportPath
        : `${exportPath}.skypack`;
      const savedPath = await invoke<string>("export_skypack_cmd", {
        instanceId: instance.id,
        targetPath: target,
      });
      setFeedback(
        language === "tr"
          ? `Tam paket (.skypack) oluşturuldu: ${savedPath}`
          : `Full bundle (.skypack) created: ${savedPath}`
      );
    } catch (err) {
      setErrorMsg(String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleExportLightweightSkycode = async () => {
    if (!instance) return;
    setBusy(true);
    setErrorMsg(null);
    setFeedback(null);
    try {
      const target = exportPath.endsWith(".skycode")
        ? exportPath
        : `${exportPath}.skycode`;
      const res = await invoke<SkyCodeExportResult>("export_skycode_cmd", {
        instanceId: instance.id,
        targetFile: target,
      });
      setSkyCodeResult(res);
      setFeedback(
        language === "tr"
          ? `Hafif reçete (${res.resolvedOnlineMods}/${res.totalMods} mod adresi) ve paylaşım kodu hazır! Dosya: ${res.savedFilePath}`
          : `Lightweight recipe (${res.resolvedOnlineMods}/${res.totalMods} mods) and share code ready!`
      );
    } catch (err) {
      setErrorMsg(String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleExportServerPack = async () => {
    if (!instance) return;
    setBusy(true);
    setErrorMsg(null);
    setFeedback(null);
    try {
      const target = `${exportPath}_ServerPack.zip`;
      const count = await invoke<number>("export_server_pack_cmd", {
        instanceId: instance.id,
        targetZipPath: target,
      });
      setFeedback(
        language === "tr"
          ? `Sunucu paketi oluşturuldu (${count} sunucu uyumlu mod, eula.txt ve start_server.bat eklendi): ${target}`
          : `Server pack created (${count} server mods): ${target}`
      );
    } catch (err) {
      setErrorMsg(String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleCopyShareCode = async () => {
    if (!skyCodeResult?.shareCode) return;
    try {
      await navigator.clipboard.writeText(skyCodeResult.shareCode);
      setCopiedCode(true);
      setTimeout(() => setCopiedCode(false), 2000);
    } catch {
      // ignore
    }
  };

  const handleFileChosen = (file: File | null | undefined) => {
    if (!file) return;
    setErrorMsg(null);
    setSelectedFile(file);
  };

  const readFileAsBase64 = (file: File): Promise<string> =>
    new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        if (typeof reader.result === "string") {
          const base64Part = reader.result.includes(",")
            ? reader.result.split(",")[1]
            : reader.result;
          resolve(base64Part);
        } else {
          reject(new Error("Dosya okunamadı."));
        }
      };
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(file);
    });

  const handleImportPack = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setErrorMsg(null);
    setFeedback(null);

    try {
      let created: InstanceConfig;

      // 1. If a file was selected via File Picker or Drag-and-Drop
      if (selectedFile) {
        const b64 = await readFileAsBase64(selectedFile);
        created = await invoke<InstanceConfig>("import_pack_from_bytes_cmd", {
          fileName: selectedFile.name,
          bytesBase64: b64,
          customName: null,
        });
        onPackImported(created);
        onClose();
        return;
      }

      // 2. Otherwise use text input (SKY1-... share code or file path)
      const cleaned = importInput.trim().replace(/^"|"$/g, "");
      if (!cleaned) {
        setErrorMsg(
          language === "tr"
            ? "Lütfen bir paket dosyası seçin / sürükleyin veya paylaşım kodu girin."
            : "Please select/drop a pack file or enter a share code."
        );
        return;
      }

      if (
        cleaned.startsWith("SKY1-") ||
        cleaned.toLowerCase().endsWith(".skycode") ||
        cleaned.startsWith("{")
      ) {
        created = await invoke<InstanceConfig>("import_skycode_cmd", {
          codeOrPath: cleaned,
        });
      } else if (cleaned.toLowerCase().endsWith(".skypack")) {
        created = await invoke<InstanceConfig>("import_skypack_cmd", {
          archivePath: cleaned,
        });
      } else {
        created = await invoke<InstanceConfig>("import_modpack_archive_cmd", {
          archivePath: cleaned,
          customName: null,
        });
      }
      onPackImported(created);
      onClose();
    } catch (err) {
      setErrorMsg(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4">
      <div className="w-full max-w-xl rounded-2xl bg-bg-card border border-border-subtle shadow-2xl overflow-hidden flex flex-col max-h-[88vh]">
        <div className="p-4 border-b border-border-subtle flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Share2 className="w-4 h-4 text-accent-primary" />
            <h2 className="text-sm font-bold text-text-primary">
              {t.shareModalTitle}
              {instance ? ` — ${instance.name}` : ""}
            </h2>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg bg-bg-elevated text-text-secondary hover:text-white"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Mode Switcher */}
        <div className="px-4 pt-3 flex gap-2 border-b border-border-subtle pb-3">
          {instance && (
            <button
              onClick={() => setActiveTab("export")}
              className={`px-3.5 py-1.5 rounded-xl text-xs font-semibold flex items-center gap-1.5 transition ${
                activeTab === "export"
                  ? "bg-accent-primary text-white"
                  : "bg-bg-elevated text-text-secondary"
              }`}
            >
              <Upload className="w-3.5 h-3.5" />
              <span>{t.tabExportPack}</span>
            </button>
          )}
          <button
            onClick={() => setActiveTab("import")}
            className={`px-3.5 py-1.5 rounded-xl text-xs font-semibold flex items-center gap-1.5 transition ${
              activeTab === "import"
                ? "bg-accent-primary text-white"
                : "bg-bg-elevated text-text-secondary"
            }`}
          >
            <Download className="w-3.5 h-3.5" />
            <span>{t.tabImportPack}</span>
          </button>
        </div>

        <div className="p-4 space-y-4 overflow-y-auto">
          {errorMsg && (
            <div className="p-3 rounded-xl bg-red-500/15 border border-red-500/30 text-xs text-red-300">
              {errorMsg}
            </div>
          )}

          {feedback && (
            <div className="p-3 rounded-xl bg-emerald-500/15 border border-emerald-500/30 text-xs text-emerald-300 break-all">
              {feedback}
            </div>
          )}

          {activeTab === "export" && instance ? (
            <div className="space-y-4">
              <div className="space-y-1">
                <label className="text-[11px] font-medium text-text-secondary">
                  Kayıt Yolu / Dosya Adı:
                </label>
                <input
                  type="text"
                  value={exportPath}
                  onChange={(e) => setExportPath(e.target.value)}
                  className="w-full px-3 py-2 rounded-xl bg-bg-secondary border border-border-subtle text-xs text-text-primary"
                />
              </div>

              {/* Option 1: Full Offline Bundle (.skypack) */}
              <div className="p-3.5 rounded-xl bg-bg-secondary border border-border-subtle space-y-2.5">
                <div className="flex items-center gap-2 text-xs font-semibold text-text-primary">
                  <Archive className="w-4 h-4 text-accent-secondary" />
                  <span>{t.exportFullTitle}</span>
                </div>
                <p className="text-[11px] text-text-secondary leading-relaxed">
                  {t.exportFullDesc}
                </p>
                <button
                  disabled={busy}
                  onClick={handleExportFullSkypack}
                  className="px-3.5 py-2 rounded-xl bg-accent-primary hover:opacity-90 disabled:opacity-50 text-white text-xs font-semibold transition"
                >
                  {t.exportFullBtn}
                </button>
              </div>

              {/* Option 2: Lightweight Recipe & Single-Line Share Code (.skycode / SKY1-...) */}
              <div className="p-3.5 rounded-xl bg-bg-secondary border border-border-subtle space-y-2.5">
                <div className="flex items-center gap-2 text-xs font-semibold text-text-primary">
                  <Code2 className="w-4 h-4 text-emerald-400" />
                  <span>{t.exportCodeTitle}</span>
                </div>
                <p className="text-[11px] text-text-secondary leading-relaxed">
                  {t.exportCodeDesc}
                </p>
                <button
                  disabled={busy}
                  onClick={handleExportLightweightSkycode}
                  className="px-3.5 py-2 rounded-xl bg-emerald-600 hover:bg-emerald-500 disabled:opacity-50 text-white text-xs font-semibold transition"
                >
                  {t.exportCodeBtn}
                </button>

                {skyCodeResult && (
                  <div className="pt-2 space-y-1.5">
                    <div className="flex items-center justify-between text-[11px] text-text-secondary">
                      <span>Tek Satırlık Paylaşım Kodu (Arkadaşına Gönder):</span>
                      <button
                        type="button"
                        onClick={handleCopyShareCode}
                        className="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-bg-elevated hover:bg-bg-card text-text-primary font-medium"
                      >
                        {copiedCode ? (
                          <>
                            <Check className="w-3 h-3 text-emerald-400" />
                            <span className="text-emerald-400">
                              {t.copiedBtn}
                            </span>
                          </>
                        ) : (
                          <>
                            <Copy className="w-3 h-3" />
                            <span>Kodu Kopyala</span>
                          </>
                        )}
                      </button>
                    </div>
                    <input
                      readOnly
                      value={skyCodeResult.shareCode}
                      className="w-full px-3 py-1.5 rounded-lg bg-bg-primary border border-border-subtle text-[11px] font-mono text-emerald-300"
                    />
                  </div>
                )}
              </div>

              {/* Option 3: Dedicated Server Pack Builder */}
              <div className="p-3.5 rounded-xl bg-bg-secondary border border-border-subtle space-y-2.5">
                <div className="flex items-center gap-2 text-xs font-semibold text-text-primary">
                  <Server className="w-4 h-4 text-amber-400" />
                  <span>{t.exportServerTitle}</span>
                </div>
                <p className="text-[11px] text-text-secondary leading-relaxed">
                  {t.exportServerDesc}
                </p>
                <button
                  disabled={busy}
                  onClick={handleExportServerPack}
                  className="px-3.5 py-2 rounded-xl bg-bg-elevated hover:bg-bg-card text-text-primary text-xs font-semibold border border-border-subtle transition"
                >
                  {t.exportServerBtn}
                </button>
              </div>
            </div>
          ) : (
            <form onSubmit={handleImportPack} className="space-y-4">
              {/* Drag and Drop + File Picker Area */}
              <div>
                <label className="text-xs font-semibold text-text-primary block mb-1.5">
                  {language === "tr"
                    ? "1. Paket Dosyası Seçin veya Buraya Sürükleyin (.skypack, .skycode, .mrpack, .zip)"
                    : "1. Choose or Drag & Drop a Pack File (.skypack, .skycode, .mrpack, .zip)"}
                </label>

                <input
                  ref={fileInputRef}
                  type="file"
                  accept=".skypack,.skycode,.mrpack,.zip"
                  onChange={(e) => handleFileChosen(e.target.files?.[0])}
                  className="hidden"
                />

                <div
                  onClick={() => fileInputRef.current?.click()}
                  onDragOver={(e) => {
                    e.preventDefault();
                    setIsDraggingFile(true);
                  }}
                  onDragLeave={() => setIsDraggingFile(false)}
                  onDrop={(e) => {
                    e.preventDefault();
                    setIsDraggingFile(false);
                    handleFileChosen(e.dataTransfer.files?.[0]);
                  }}
                  className={`p-5 rounded-2xl border-2 border-dashed flex flex-col items-center justify-center text-center cursor-pointer transition ${
                    isDraggingFile
                      ? "border-accent-primary bg-accent-primary/15"
                      : selectedFile
                      ? "border-emerald-500/60 bg-emerald-500/10"
                      : "border-border-subtle bg-bg-secondary hover:border-accent-primary/60"
                  }`}
                >
                  {selectedFile ? (
                    <div className="space-y-1.5">
                      <div className="w-10 h-10 rounded-xl bg-emerald-500/20 text-emerald-300 flex items-center justify-center mx-auto">
                        <Check className="w-5 h-5" />
                      </div>
                      <div className="text-xs font-bold text-text-primary">
                        {selectedFile.name}
                      </div>
                      <div className="text-[11px] text-text-secondary">
                        {(selectedFile.size / (1024 * 1024)).toFixed(2)} MB
                      </div>
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          setSelectedFile(null);
                        }}
                        className="text-[11px] text-red-400 hover:underline pt-1"
                      >
                        {language === "tr" ? "Seçimi Kaldır" : "Clear Selection"}
                      </button>
                    </div>
                  ) : (
                    <>
                      <div className="w-10 h-10 rounded-xl bg-accent-primary/15 text-accent-secondary flex items-center justify-center mb-2">
                        <FileUp className="w-5 h-5" />
                      </div>
                      <div className="text-xs font-semibold text-text-primary">
                        {language === "tr"
                          ? "Dosya Seçmek İçin Tıklayın veya Dosyayı Buraya Sürükleyin"
                          : "Click to Browse or Drag & Drop Pack File Here"}
                      </div>
                      <p className="text-[11px] text-text-muted mt-1">
                        .skypack • .skycode • .mrpack (Modrinth) • .zip (CurseForge)
                      </p>
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          fileInputRef.current?.click();
                        }}
                        className="mt-2.5 px-3 py-1.5 rounded-xl bg-bg-elevated hover:bg-bg-card text-xs font-medium text-text-primary inline-flex items-center gap-1.5 border border-border-subtle"
                      >
                        <FolderOpen className="w-3.5 h-3.5 text-accent-secondary" />
                        <span>
                          {language === "tr" ? "Bilgisayardan Dosya Seç" : "Choose File"}
                        </span>
                      </button>
                    </>
                  )}
                </div>
              </div>

              {/* Share Code / Path Input */}
              <div className="space-y-1.5">
                <label className="text-xs font-semibold text-text-primary block">
                  {language === "tr"
                    ? "2. Veya Tek Satırlık Paylaşım Kodu (SKY1-...) Yapıştırın"
                    : "2. Or Paste a Single-Line Share Code (SKY1-...)"}
                </label>
                <textarea
                  rows={3}
                  value={importInput}
                  onChange={(e) => setImportInput(e.target.value)}
                  placeholder={t.importCodePlaceholder}
                  className="w-full p-3 rounded-xl bg-bg-secondary border border-border-subtle text-xs font-mono text-text-primary focus:outline-none focus:border-accent-primary"
                />
              </div>

              <button
                type="submit"
                disabled={busy}
                className="w-full py-2.5 rounded-xl bg-accent-primary hover:opacity-90 disabled:opacity-50 text-white text-xs font-semibold flex items-center justify-center gap-2 transition"
              >
                <Download className="w-4 h-4" />
                <span>
                  {busy
                    ? language === "tr"
                      ? "Paket Kuruluyor ve Modlar Hazırlanıyor..."
                      : "Importing Pack..."
                    : t.importNowBtn}
                </span>
              </button>
            </form>
          )}
        </div>
      </div>
    </div>
  );
};
