import { invoke } from "@tauri-apps/api/core";
import { ImagePlus, Plus, Trash2 } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";
import type {
  InstanceConfig,
  ModloaderType,
  ModloaderVersionItem,
  MojangManifest,
  MojangVersionEntry,
} from "../hooks/useLauncher";
import { Language, translations } from "../i18n/translations";

interface VersionDownloaderProps {
  onInstanceCreated: (inst: InstanceConfig) => void;
  language: Language;
}

const FALLBACK_VERSIONS: MojangVersionEntry[] = [
  { id: "1.21.1", type: "release", url: "", time: "", releaseTime: "" },
  { id: "1.20.6", type: "release", url: "", time: "", releaseTime: "" },
  { id: "1.20.4", type: "release", url: "", time: "", releaseTime: "" },
  { id: "1.20.1", type: "release", url: "", time: "", releaseTime: "" },
  { id: "1.19.4", type: "release", url: "", time: "", releaseTime: "" },
  { id: "1.18.2", type: "release", url: "", time: "", releaseTime: "" },
  { id: "1.16.5", type: "release", url: "", time: "", releaseTime: "" },
  { id: "1.12.2", type: "release", url: "", time: "", releaseTime: "" },
  { id: "1.8.9", type: "release", url: "", time: "", releaseTime: "" },
];

export const VersionDownloader: React.FC<VersionDownloaderProps> = ({
  onInstanceCreated,
  language,
}) => {
  const t = translations[language];
  const [instanceName, setInstanceName] = useState("");
  const [iconDataUrl, setIconDataUrl] = useState<string | null>(null);
  const [isDraggingLogo, setIsDraggingLogo] = useState(false);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const [filterType, setFilterType] = useState<
    "release" | "snapshot" | "historical"
  >("release");
  const [versions, setVersions] =
    useState<MojangVersionEntry[]>(FALLBACK_VERSIONS);
  const [selectedMcVersion, setSelectedMcVersion] = useState("1.21.1");
  const [selectedModloader, setSelectedModloader] =
    useState<ModloaderType>("vanilla");
  const [loaderVersions, setLoaderVersions] = useState<ModloaderVersionItem[]>(
    []
  );
  const [selectedLoaderVersion, setSelectedLoaderVersion] = useState("");
  const [overrideMemory, setOverrideMemory] = useState(false);
  const [maxRamMb, setMaxRamMb] = useState(4096);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<MojangManifest>("get_mojang_versions", { filterType })
      .then((manifest) => {
        if (manifest.versions && manifest.versions.length > 0) {
          setVersions(manifest.versions.slice(0, 100));
          setSelectedMcVersion(manifest.versions[0].id);
        }
      })
      .catch(() => {});
  }, [filterType]);

  useEffect(() => {
    if (selectedModloader === "vanilla") {
      setLoaderVersions([]);
      setSelectedLoaderVersion("");
      return;
    }
    invoke<ModloaderVersionItem[]>("get_modloader_versions", {
      loaderType: selectedModloader,
      mcVersion: selectedMcVersion,
    })
      .then((items) => {
        setLoaderVersions(items);
        setSelectedLoaderVersion(items[0]?.loaderVersion || "");
      })
      .catch(() => {
        setLoaderVersions([]);
        setSelectedLoaderVersion("");
      });
  }, [selectedModloader, selectedMcVersion]);

  const handleLogoFile = (file: File | null | undefined) => {
    if (!file) return;
    if (!file.type.startsWith("image/")) {
      setError(
        language === "tr"
          ? "Lütfen geçerli bir görsel dosyası (.png, .jpg, .webp, .gif) seçin."
          : "Please select a valid image file (.png, .jpg, .webp, .gif)."
      );
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result === "string") {
        setIconDataUrl(reader.result);
      }
    };
    reader.readAsDataURL(file);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    const finalName =
      instanceName.trim() ||
      `${
        selectedModloader === "vanilla"
          ? "Minecraft"
          : selectedModloader.toUpperCase()
      } ${selectedMcVersion}`;

    try {
      const created = await invoke<InstanceConfig>("create_instance", {
        name: finalName,
        mcVersion: selectedMcVersion,
        modloader: selectedModloader,
        modloaderVersion:
          selectedModloader === "vanilla" ? null : selectedLoaderVersion,
        overrideMemory,
        minRamMb: overrideMemory ? 1024 : null,
        maxRamMb: overrideMemory ? maxRamMb : null,
        iconDataUrl: iconDataUrl || null,
      });
      onInstanceCreated(created);
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <div className="flex-1 h-screen overflow-y-auto p-6">
      <div className="max-w-xl mx-auto space-y-5">
        <h2 className="text-lg font-bold text-text-primary">
          {t.versionCreatorTitle}
        </h2>

        <form
          onSubmit={handleSubmit}
          className="p-5 rounded-2xl bg-bg-card border border-border-subtle space-y-4"
        >
          {/* Pack Visual Logo + Name Row */}
          <div className="flex items-start gap-4">
            <div className="shrink-0">
              <label className="block text-xs font-medium text-text-secondary mb-1.5">
                {language === "tr" ? "Paket Logosu" : "Pack Logo"}
              </label>
              <input
                ref={fileInputRef}
                type="file"
                accept="image/png,image/jpeg,image/webp,image/gif"
                onChange={(e) => handleLogoFile(e.target.files?.[0])}
                className="hidden"
              />
              <div
                onClick={() => fileInputRef.current?.click()}
                onDragOver={(e) => {
                  e.preventDefault();
                  setIsDraggingLogo(true);
                }}
                onDragLeave={() => setIsDraggingLogo(false)}
                onDrop={(e) => {
                  e.preventDefault();
                  setIsDraggingLogo(false);
                  handleLogoFile(e.dataTransfer.files?.[0]);
                }}
                className={`relative w-16 h-16 rounded-2xl border-2 border-dashed flex flex-col items-center justify-center cursor-pointer overflow-hidden transition ${
                  isDraggingLogo
                    ? "border-accent-primary bg-accent-primary/15"
                    : "border-border-subtle bg-bg-secondary hover:border-accent-primary/60"
                }`}
                title={
                  language === "tr"
                    ? "Logo seçmek için tıklayın veya görseli sürükleyin"
                    : "Click or drag & drop an image logo"
                }
              >
                {iconDataUrl ? (
                  <>
                    <img
                      src={iconDataUrl}
                      alt="Pack Logo"
                      className="w-full h-full object-cover"
                    />
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        setIconDataUrl(null);
                      }}
                      className="absolute top-1 right-1 p-1 rounded-md bg-black/70 text-red-300 hover:text-white"
                      title={language === "tr" ? "Logoyu Kaldır" : "Remove Logo"}
                    >
                      <Trash2 className="w-3 h-3" />
                    </button>
                  </>
                ) : (
                  <>
                    <ImagePlus className="w-5 h-5 text-accent-secondary" />
                    <span className="text-[9px] text-text-muted mt-1">
                      {language === "tr" ? "Logo Seç" : "Add Logo"}
                    </span>
                  </>
                )}
              </div>
            </div>

            <div className="flex-1">
              <label className="block text-xs font-medium text-text-secondary mb-1.5">
                {t.instanceNameLabel}
              </label>
              <input
                type="text"
                value={instanceName}
                onChange={(e) => setInstanceName(e.target.value)}
                placeholder={`Minecraft ${selectedMcVersion}`}
                className="w-full px-3.5 py-2.5 rounded-xl bg-bg-secondary border border-border-subtle text-sm text-text-primary focus:outline-none focus:border-accent-primary"
              />
              <p className="text-[11px] text-text-muted mt-1.5">
                {language === "tr"
                  ? "Sol taraftaki kutuya tıklayarak veya görsel sürükleyerek paketinize özel logo ekleyebilirsiniz."
                  : "Click or drag an image onto the left box to set a custom pack logo."}
              </p>
            </div>
          </div>

          {/* Version Filter & Selection */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium text-text-secondary mb-1.5">
                {t.mcVersionFilter}
              </label>
              <select
                value={filterType}
                onChange={(e) =>
                  setFilterType(
                    e.target.value as "release" | "snapshot" | "historical"
                  )
                }
                className="w-full px-3 py-2 rounded-xl bg-bg-secondary border border-border-subtle text-sm text-text-primary focus:outline-none focus:border-accent-primary"
              >
                <option value="release">{t.filterRelease}</option>
                <option value="snapshot">{t.filterSnapshot}</option>
                <option value="historical">{t.filterHistorical}</option>
              </select>
            </div>

            <div>
              <label className="block text-xs font-medium text-text-secondary mb-1.5">
                {t.selectMcVersion}
              </label>
              <select
                value={selectedMcVersion}
                onChange={(e) => setSelectedMcVersion(e.target.value)}
                className="w-full px-3 py-2 rounded-xl bg-bg-secondary border border-border-subtle text-sm text-text-primary focus:outline-none focus:border-accent-primary"
              >
                {versions.map((v) => (
                  <option key={v.id} value={v.id}>
                    {v.id}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {/* Modloader Selection */}
          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1.5">
              {t.selectModloader}
            </label>
            <div className="grid grid-cols-5 gap-1.5">
              {(
                ["vanilla", "fabric", "forge", "neoforge", "quilt"] as const
              ).map((loader) => (
                <button
                  type="button"
                  key={loader}
                  onClick={() => setSelectedModloader(loader)}
                  className={`py-2 px-2 rounded-xl text-xs font-medium capitalize border transition ${
                    selectedModloader === loader
                      ? "bg-accent-primary text-white border-accent-primary"
                      : "bg-bg-secondary text-text-secondary border-border-subtle hover:bg-bg-elevated"
                  }`}
                >
                  {loader}
                </button>
              ))}
            </div>
          </div>

          {selectedModloader !== "vanilla" && loaderVersions.length > 0 && (
            <div>
              <label className="block text-xs font-medium text-text-secondary mb-1.5">
                {t.selectLoaderVersion}
              </label>
              <select
                value={selectedLoaderVersion}
                onChange={(e) => setSelectedLoaderVersion(e.target.value)}
                className="w-full px-3 py-2 rounded-xl bg-bg-secondary border border-border-subtle text-sm text-text-primary"
              >
                {loaderVersions.map((lv) => (
                  <option key={lv.loaderVersion} value={lv.loaderVersion}>
                    {lv.loaderVersion}
                  </option>
                ))}
              </select>
            </div>
          )}

          {/* Optional Custom RAM */}
          <div className="pt-2 border-t border-border-subtle space-y-2.5">
            <label className="flex items-center gap-2 text-xs font-medium text-text-primary cursor-pointer">
              <input
                type="checkbox"
                checked={overrideMemory}
                onChange={(e) => setOverrideMemory(e.target.checked)}
                className="rounded accent-sky-500"
              />
              <span>{t.customMemoryToggle}</span>
            </label>

            {overrideMemory && (
              <div className="p-3 rounded-xl bg-bg-secondary border border-border-subtle space-y-1.5">
                <div className="flex justify-between text-xs">
                  <span className="text-text-secondary">RAM</span>
                  <span className="font-semibold text-text-primary">
                    {maxRamMb} MB ({(maxRamMb / 1024).toFixed(1)} GB)
                  </span>
                </div>
                <input
                  type="range"
                  min={2048}
                  max={16384}
                  step={512}
                  value={maxRamMb}
                  onChange={(e) => setMaxRamMb(Number(e.target.value))}
                  className="w-full accent-sky-500"
                />
              </div>
            )}
          </div>

          {error && (
            <div className="p-3 rounded-xl bg-red-500/10 border border-red-500/25 text-xs text-red-300">
              {error}
            </div>
          )}

          <button
            type="submit"
            className="w-full py-2.5 px-4 rounded-xl bg-accent-primary hover:opacity-90 text-white font-semibold text-xs flex items-center justify-center gap-1.5 transition"
          >
            <Plus className="w-4 h-4" />
            <span>{t.createInstanceBtn}</span>
          </button>
        </form>
      </div>
    </div>
  );
};
