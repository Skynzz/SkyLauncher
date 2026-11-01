import { invoke } from "@tauri-apps/api/core";
import {
  BookOpen,
  Calendar,
  Check,
  Code2,
  Download,
  ExternalLink,
  Eye,
  Heart,
  Layers,
  LayoutGrid,
  List,
  Lock,
  Maximize2,
  Package,
  Search,
  ShieldCheck,
  Sparkles,
  Tag,
  Unlock,
  X,
  ZoomIn,
} from "lucide-react";
import React, { useCallback, useEffect, useRef, useState } from "react";
import type {
  InstanceConfig,
  StoreContentType,
  StoreProjectDetails,
  StoreProviderType,
  StoreSearchItem,
} from "../hooks/useLauncher";
import { Language, translations } from "../i18n/translations";

interface ContentBrowserProps {
  instances: InstanceConfig[];
  lockedInstance: InstanceConfig | null;
  onSelectTargetInstance: (inst: InstanceConfig | null) => void;
  onModpackInstalled: (created: InstanceConfig) => void;
  language: Language;
}

const STORE_CATEGORIES: { id: string; tr: string; en: string }[] = [
  { id: "all", tr: "Tümü", en: "All" },
  { id: "technology", tr: "Teknoloji", en: "Technology" },
  { id: "magic", tr: "Büyü", en: "Magic" },
  { id: "adventure", tr: "Macera & RPG", en: "Adventure & RPG" },
  { id: "optimization", tr: "Optimizasyon (FPS)", en: "Optimization" },
  { id: "worldgen", tr: "Dünya Oluşumu", en: "World Gen" },
  { id: "decoration", tr: "Dekorasyon", en: "Decoration" },
  { id: "storage", tr: "Depolama", en: "Storage" },
  { id: "utility", tr: "Araçlar", en: "Utility" },
  { id: "quests", tr: "Görevli (Quests)", en: "Quests" },
  { id: "equipment", tr: "Silah & Zırh", en: "Equipment" },
  { id: "mobs", tr: "Yaratıklar", en: "Mobs" },
  { id: "library", tr: "Kütüphane / API", en: "Library" },
];

function normalizeModKey(str: string): string {
  return str
    .toLowerCase()
    .replace(/\s*\((fabric|forge|neoforge|quilt)\)\s*$/i, "")
    .replace(/\s*\[(fabric|forge|neoforge|quilt|fabric\/forge)\]\s*$/i, "")
    .replace(/[^a-z0-9]/g, "");
}

function formatNumber(num: number): string {
  if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(1)}M`;
  if (num >= 1_000) return `${(num / 1_000).toFixed(1)}K`;
  return String(num);
}

/**
 * Converts mixed Modrinth Markdown + HTML into clean, safe HTML
 * so descriptions render properly without broken raw tags or Markdown syntax.
 */
function renderMixedMarkdownAndHtml(raw: string): string {
  if (!raw) return "";

  let html = raw
    // Remove script/style/iframe tags for safety
    .replace(/<script[\s\S]*?>[\s\S]*?<\/script>/gi, "")
    .replace(/<style[\s\S]*?>[\s\S]*?<\/style>/gi, "")
    .replace(/<iframe[\s\S]*?>[\s\S]*?<\/iframe>/gi, "");

  // Fenced code blocks ```...```
  html = html.replace(/```([a-zA-Z0-9_-]*)\n([\s\S]*?)```/g, (_m, _lang, code) => {
    const escaped = String(code)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
    return `<pre class="my-3 p-3 rounded-xl bg-black/40 border border-white/10 text-xs font-mono overflow-x-auto text-slate-200"><code>${escaped}</code></pre>`;
  });

  // Linked images [![alt](imgUrl)](linkUrl) -> render clickable image for lightbox zoom
  html = html.replace(
    /\[!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)\]\(([^)]+)\)/g,
    (_m, alt, imgUrl) =>
      `<img src="${imgUrl}" alt="${alt || "Mod Görseli"}" class="inline-block max-w-full rounded-xl my-1.5 cursor-zoom-in hover:opacity-90 transition border border-white/10" />`
  );

  // Standard Markdown images ![alt](imgUrl)
  html = html.replace(
    /!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g,
    (_m, alt, imgUrl) =>
      `<img src="${imgUrl}" alt="${alt || "Mod Görseli"}" class="max-w-full rounded-xl my-2 cursor-zoom-in hover:opacity-90 transition border border-white/10" />`
  );

  // Markdown links [label](url)
  html = html.replace(
    /\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g,
    (_m, label, url) =>
      `<a href="${url}" target="_blank" rel="noreferrer" class="text-sky-400 hover:underline font-medium">${label}</a>`
  );

  // Process line-by-line for Markdown headings, lists, blockquotes, hr
  const lines = html.split(/\r?\n/);
  const processedLines = lines.map((line) => {
    const trimmed = line.trim();
    if (!trimmed) return "<div class='h-2'></div>";

    if (/^#{4}\s+(.+)$/.test(trimmed)) {
      return trimmed.replace(
        /^#{4}\s+(.+)$/,
        '<h4 class="text-xs font-bold text-white mt-3 mb-1">$1</h4>'
      );
    }
    if (/^#{3}\s+(.+)$/.test(trimmed)) {
      return trimmed.replace(
        /^#{3}\s+(.+)$/,
        '<h3 class="text-sm font-bold text-white mt-4 mb-1.5">$1</h3>'
      );
    }
    if (/^#{2}\s+(.+)$/.test(trimmed)) {
      return trimmed.replace(
        /^#{2}\s+(.+)$/,
        '<h2 class="text-base font-bold text-white mt-4 mb-2 pb-1 border-b border-white/10">$1</h2>'
      );
    }
    if (/^#\s+(.+)$/.test(trimmed)) {
      return trimmed.replace(
        /^#\s+(.+)$/,
        '<h1 class="text-lg font-extrabold text-white mt-4 mb-2 pb-1 border-b border-white/10">$1</h1>'
      );
    }
    if (/^(-{3,}|\*{3,}|_{3,})$/.test(trimmed)) {
      return '<hr class="my-3 border-white/10" />';
    }
    if (/^>\s*(.+)$/.test(trimmed)) {
      return trimmed.replace(
        /^>\s*(.+)$/,
        '<blockquote class="border-l-2 border-sky-500/60 pl-3 py-1 my-1.5 text-slate-300 italic bg-white/5 rounded-r-lg">$1</blockquote>'
      );
    }
    if (/^[-*+]\s+(.+)$/.test(trimmed)) {
      return trimmed.replace(
        /^[-*+]\s+(.+)$/,
        '<div class="flex items-start gap-2 my-1 pl-1"><span class="text-sky-400 mt-1">•</span><span class="flex-1">$1</span></div>'
      );
    }
    return line;
  });

  html = processedLines.join("\n");

  // Inline bold, italic, strikethrough, inline code
  html = html
    .replace(/\*\*([^*]+)\*\*/g, '<strong class="text-white font-semibold">$1</strong>')
    .replace(/__([^_]+)__/g, '<strong class="text-white font-semibold">$1</strong>')
    .replace(/~~([^~]+)~~/g, '<del class="opacity-60">$1</del>')
    .replace(
      /`([^`\n]+)`/g,
      '<code class="px-1.5 py-0.5 rounded bg-black/40 border border-white/10 text-[11px] font-mono text-sky-300">$1</code>'
    );

  // Style raw HTML <img> tags so they also show zoom cursor and rounded corners
  html = html.replace(
    /<img\s+([^>]*?)>/gi,
    '<img $1 class="max-w-full rounded-xl my-2 cursor-zoom-in hover:opacity-90 transition inline-block" />'
  );

  return html;
}

export const ContentBrowser: React.FC<ContentBrowserProps> = ({
  instances,
  lockedInstance,
  onSelectTargetInstance,
  onModpackInstalled,
  language,
}) => {
  const t = translations[language];
  const [query, setQuery] = useState("");
  const [contentType, setContentType] = useState<StoreContentType>(
    lockedInstance ? "mod" : "modpack"
  );
  const [provider, setProvider] = useState<StoreProviderType>("both");
  const [sortBy, setSortBy] = useState<"downloads" | "updated" | "relevance">(
    "downloads"
  );
  const [selectedCategory, setSelectedCategory] = useState<string>("all");
  const [viewMode, setViewMode] = useState<"grid" | "list">("grid");
  const [isAutoLocked, setIsAutoLocked] = useState<boolean>(!!lockedInstance);

  const [results, setResults] = useState<StoreSearchItem[]>([]);
  const [offset, setOffset] = useState<number>(0);
  const [hasMore, setHasMore] = useState<boolean>(true);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);

  const [installingId, setInstallingId] = useState<string | null>(null);
  const [installedIds, setInstalledIds] = useState<Record<string, boolean>>({});
  const [statusBanner, setStatusBanner] = useState<string | null>(null);

  const [inspectingProject, setInspectingProject] =
    useState<StoreProjectDetails | null>(null);
  const [inspectorLoading, setInspectorLoading] = useState(false);
  const [selectedVersionId, setSelectedVersionId] = useState<string>("");
  const [zoomedImageUrl, setZoomedImageUrl] = useState<string | null>(null);

  const scrollContainerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (lockedInstance) {
      setIsAutoLocked(true);
      if (contentType === "modpack") {
        setContentType("mod");
      }
    }
  }, [lockedInstance]);

  // Reset category if switching to resourcepack or shader where mod categories don't apply
  useEffect(() => {
    if (contentType === "resourcepack" || contentType === "shader") {
      setSelectedCategory("all");
    }
  }, [contentType]);

  const fetchPage = useCallback(
    async (pageOffset: number, append: boolean) => {
      if (append) {
        setLoadingMore(true);
      } else {
        setLoading(true);
        setStatusBanner(null);
      }

      try {
        const effectiveMcVer =
          isAutoLocked && lockedInstance && contentType !== "modpack"
            ? lockedInstance.mcVersion
            : null;
        const effectiveLoader =
          isAutoLocked && lockedInstance && contentType === "mod"
            ? lockedInstance.modloader
            : null;

        const list = await invoke<StoreSearchItem[]>("search_store", {
          query,
          contentType,
          provider,
          sortBy,
          mcVersion: effectiveMcVer,
          loader: effectiveLoader,
          category: selectedCategory === "all" ? null : selectedCategory,
          offset: pageOffset,
        });

        if (list.length < 12) {
          setHasMore(false);
        } else {
          setHasMore(true);
        }

        setResults((prev) => {
          const base = append ? [...prev] : [];
          const seenKeys = new Set<string>();
          for (const existing of base) {
            const tk = normalizeModKey(existing.title);
            const sk = normalizeModKey(existing.slug);
            if (tk) seenKeys.add(tk);
            if (sk) seenKeys.add(sk);
            seenKeys.add(`${existing.provider}:${existing.projectId}`);
          }

          for (const item of list) {
            const tk = normalizeModKey(item.title);
            const sk = normalizeModKey(item.slug);
            const idKey = `${item.provider}:${item.projectId}`;
            if (
              seenKeys.has(idKey) ||
              (tk && seenKeys.has(tk)) ||
              (sk && seenKeys.has(sk))
            ) {
              continue;
            }
            if (tk) seenKeys.add(tk);
            if (sk) seenKeys.add(sk);
            seenKeys.add(idKey);
            base.push(item);
          }
          return base;
        });
      } catch (err) {
        setStatusBanner(String(err));
      } finally {
        setLoading(false);
        setLoadingMore(false);
      }
    },
    [
      query,
      contentType,
      provider,
      sortBy,
      selectedCategory,
      isAutoLocked,
      lockedInstance,
    ]
  );

  // Trigger fresh search whenever filters change
  useEffect(() => {
    setOffset(0);
    setHasMore(true);
    fetchPage(0, false);
  }, [fetchPage]);

  // Infinite scroll handler
  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
    if (loading || loadingMore || !hasMore) return;
    const target = e.currentTarget;
    const distanceToBottom =
      target.scrollHeight - target.scrollTop - target.clientHeight;
    if (distanceToBottom < 280) {
      const nextOffset = offset + 24;
      setOffset(nextOffset);
      fetchPage(nextOffset, true);
    }
  };

  const handleOpenInspector = async (item: StoreSearchItem) => {
    setStatusBanner(null);
    setInspectorLoading(true);
    try {
      const effectiveMcVer =
        isAutoLocked && lockedInstance && item.contentType !== "modpack"
          ? lockedInstance.mcVersion
          : null;
      const effectiveLoader =
        isAutoLocked && lockedInstance && item.contentType === "mod"
          ? lockedInstance.modloader
          : null;

      const details = await invoke<StoreProjectDetails>("get_store_project", {
        projectId: item.projectId,
        provider: item.provider,
        contentType: item.contentType,
        mcVersion: effectiveMcVer,
        loader: effectiveLoader,
      });
      // Preserve author & downloads from search card if API detail doesn't include author name
      if (details.author === "Modrinth" && item.author) {
        details.author = item.author;
      }
      if (!details.downloads && item.downloads) {
        details.downloads = item.downloads;
      }
      setInspectingProject(details);
      if (details.versions.length > 0) {
        setSelectedVersionId(details.versions[0].versionId);
      } else {
        setSelectedVersionId("");
      }
    } catch (err) {
      setStatusBanner(String(err));
    } finally {
      setInspectorLoading(false);
    }
  };

  const handleInstallItem = async (
    projectId: string,
    itemProvider: string,
    itemType: StoreContentType,
    versionId?: string
  ) => {
    setStatusBanner(null);
    setInstallingId(projectId);
    try {
      if (itemType === "modpack") {
        const created = await invoke<InstanceConfig>(
          "install_store_modpack_cmd",
          {
            projectId,
            provider: itemProvider,
            versionId: versionId || null,
          }
        );
        setInstalledIds((prev) => ({ ...prev, [projectId]: true }));
        onModpackInstalled(created);
        return;
      }

      const targetInst = lockedInstance || instances[0];
      if (!targetInst) {
        setStatusBanner(
          language === "tr"
            ? "Lütfen önce bir paket oluşturun veya seçin."
            : "Please create or select a pack first."
        );
        return;
      }

      const installedFiles = await invoke<string[]>("install_store_item", {
        instanceId: targetInst.id,
        projectId,
        provider: itemProvider,
        contentType: itemType,
        versionId: versionId || null,
      });
      setInstalledIds((prev) => ({ ...prev, [projectId]: true }));
      setStatusBanner(
        language === "tr"
          ? `${targetInst.name} paketine kuruldu (${installedFiles.length} dosya + bağımlılıklar).`
          : `Installed into ${targetInst.name} (${installedFiles.length} files + dependencies).`
      );
    } catch (err) {
      setStatusBanner(String(err));
    } finally {
      setInstallingId(null);
    }
  };

  // Click handler inside Rich Description to zoom images on left-click
  const handleDescriptionClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    if (target.tagName === "IMG") {
      e.preventDefault();
      e.stopPropagation();
      const src = (target as HTMLImageElement).src;
      if (src) {
        setZoomedImageUrl(src);
      }
    }
  };

  return (
    <div
      ref={scrollContainerRef}
      onScroll={handleScroll}
      className="flex-1 h-screen overflow-y-auto p-6 space-y-4"
    >
      {/* Header & Target Pack Selector */}
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h1 className="text-lg font-bold text-text-primary flex items-center gap-2">
            <Sparkles className="w-5 h-5 text-accent-primary" />
            <span>{t.storeTitle}</span>
          </h1>
        </div>

        {/* Context-Aware Pack Lock Selector */}
        {contentType !== "modpack" && instances.length > 0 && (
          <div className="flex items-center gap-2 bg-bg-card border border-border-subtle px-3 py-1.5 rounded-xl text-xs">
            <span className="text-text-secondary">{t.storeLockedToPack}</span>
            <select
              value={lockedInstance?.id || instances[0]?.id || ""}
              onChange={(e) => {
                const found =
                  instances.find((i) => i.id === e.target.value) || null;
                onSelectTargetInstance(found);
                setIsAutoLocked(true);
              }}
              className="bg-bg-elevated text-text-primary font-semibold px-2 py-1 rounded-lg border border-border-subtle focus:outline-none"
            >
              {instances.map((inst) => (
                <option key={inst.id} value={inst.id}>
                  {inst.name} ({inst.mcVersion} • {inst.modloader})
                </option>
              ))}
            </select>

            <button
              onClick={() => setIsAutoLocked(!isAutoLocked)}
              className={`flex items-center gap-1 px-2 py-1 rounded-lg text-[11px] font-medium transition ${
                isAutoLocked
                  ? "bg-accent-primary/20 text-accent-secondary"
                  : "bg-bg-elevated text-text-muted"
              }`}
              title={t.storeClearLock}
            >
              {isAutoLocked ? (
                <>
                  <Lock className="w-3 h-3" />
                  <span>
                    {(lockedInstance || instances[0])?.mcVersion} /{" "}
                    {(lockedInstance || instances[0])?.modloader}
                  </span>
                </>
              ) : (
                <>
                  <Unlock className="w-3 h-3" />
                  <span>{language === "tr" ? "Tümü" : "All"}</span>
                </>
              )}
            </button>
          </div>
        )}
      </div>

      {/* Content Type, Search, Provider, Sort & View Mode Toggle */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-1.5 bg-bg-card p-1 rounded-xl border border-border-subtle">
          {(
            [
              ["modpack", t.storeTypeModpack],
              ["mod", t.storeTypeMod],
              ["resourcepack", t.storeTypeResourcePack],
              ["shader", t.storeTypeShader],
            ] as [StoreContentType, string][]
          ).map(([typeKey, label]) => (
            <button
              key={typeKey}
              onClick={() => setContentType(typeKey)}
              className={`px-3 py-1.5 rounded-lg text-xs font-medium transition ${
                contentType === typeKey
                  ? "bg-accent-primary text-white"
                  : "text-text-secondary hover:text-text-primary"
              }`}
            >
              {label}
            </button>
          ))}
        </div>

        <div className="flex items-center gap-2 flex-1 max-w-2xl">
          <div className="relative flex-1">
            <Search className="w-4 h-4 text-text-muted absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && fetchPage(0, false)}
              placeholder={t.storeSearchPlaceholder}
              className="w-full pl-9 pr-3 py-2 rounded-xl bg-bg-card border border-border-subtle text-xs text-text-primary focus:outline-none focus:border-accent-primary"
            />
          </div>

          <select
            value={provider}
            onChange={(e) => setProvider(e.target.value as StoreProviderType)}
            className="px-3 py-2 rounded-xl bg-bg-card border border-border-subtle text-xs text-text-primary"
          >
            <option value="both">Modrinth + CurseForge</option>
            <option value="modrinth">Modrinth</option>
            <option value="curseforge">CurseForge</option>
          </select>

          <select
            value={sortBy}
            onChange={(e) =>
              setSortBy(e.target.value as "downloads" | "updated" | "relevance")
            }
            className="px-3 py-2 rounded-xl bg-bg-card border border-border-subtle text-xs text-text-primary"
          >
            <option value="downloads">{t.storeSortDownloads}</option>
            <option value="updated">{t.storeSortUpdated}</option>
            <option value="relevance">{t.storeSortRelevance}</option>
          </select>

          {/* Grid vs List View Toggle */}
          <div className="flex items-center bg-bg-card p-1 rounded-xl border border-border-subtle shrink-0">
            <button
              onClick={() => setViewMode("grid")}
              className={`p-1.5 rounded-lg transition ${
                viewMode === "grid"
                  ? "bg-accent-primary text-white"
                  : "text-text-secondary hover:text-text-primary"
              }`}
              title={language === "tr" ? "Izgara Görünümü" : "Grid View"}
            >
              <LayoutGrid className="w-3.5 h-3.5" />
            </button>
            <button
              onClick={() => setViewMode("list")}
              className={`p-1.5 rounded-lg transition ${
                viewMode === "list"
                  ? "bg-accent-primary text-white"
                  : "text-text-secondary hover:text-text-primary"
              }`}
              title={language === "tr" ? "Liste Görünümü" : "List View"}
            >
              <List className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      </div>

      {/* Category Pills for Mods and Modpacks */}
      {(contentType === "mod" || contentType === "modpack") && (
        <div className="flex items-center gap-1.5 overflow-x-auto pb-1">
          <Tag className="w-3.5 h-3.5 text-text-muted shrink-0 mr-1" />
          {STORE_CATEGORIES.map((cat) => {
            const active = selectedCategory === cat.id;
            return (
              <button
                key={cat.id}
                onClick={() => setSelectedCategory(cat.id)}
                className={`px-3 py-1 rounded-full text-[11px] font-medium whitespace-nowrap transition ${
                  active
                    ? "bg-accent-primary text-white shadow-sm"
                    : "bg-bg-card border border-border-subtle text-text-secondary hover:text-text-primary hover:border-accent-primary/40"
                }`}
              >
                {language === "tr" ? cat.tr : cat.en}
              </button>
            );
          })}
        </div>
      )}

      {statusBanner && (
        <div className="p-3 rounded-xl bg-accent-primary/15 border border-accent-primary/30 text-xs text-text-primary flex items-center justify-between">
          <span>{statusBanner}</span>
          <button
            onClick={() => setStatusBanner(null)}
            className="text-text-secondary hover:text-white"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      )}

      {/* Results Area (Grid or List) */}
      {loading ? (
        <div className="py-16 text-center text-xs text-text-secondary">
          {language === "tr" ? "İçerikler yükleniyor..." : "Loading store content..."}
        </div>
      ) : results.length === 0 ? (
        <div className="py-16 text-center text-xs text-text-muted">
          {language === "tr"
            ? "Aradığınız kriterlere uygun mod veya paket bulunamadı."
            : "No matching mods or modpacks found."}
        </div>
      ) : (
        <>
          <div
            className={
              viewMode === "grid"
                ? "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4"
                : "flex flex-col gap-2.5"
            }
          >
            {results.map((item) => {
              const isInstalling = installingId === item.projectId;
              const isInstalled = !!installedIds[item.projectId];

              if (viewMode === "list") {
                return (
                  <div
                    key={item.provider + "-" + item.projectId}
                    onClick={() => handleOpenInspector(item)}
                    className="rounded-2xl bg-bg-card border border-border-subtle px-4 py-3 flex items-center justify-between gap-4 hover:border-accent-primary/50 cursor-pointer transition"
                  >
                    <div className="flex items-center gap-3.5 min-w-0 flex-1">
                      {item.iconUrl ? (
                        <img
                          src={item.iconUrl}
                          alt={item.title}
                          className="w-12 h-12 rounded-xl object-cover bg-bg-elevated shrink-0"
                        />
                      ) : (
                        <div className="w-12 h-12 rounded-xl bg-bg-elevated flex items-center justify-center text-accent-secondary shrink-0">
                          <Package className="w-6 h-6" />
                        </div>
                      )}

                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <h3 className="font-semibold text-sm text-text-primary truncate">
                            {item.title}
                          </h3>
                          <span className="px-2 py-0.5 rounded bg-bg-elevated text-[10px] font-medium text-text-secondary uppercase shrink-0">
                            {item.provider}
                          </span>
                          <span className="text-[11px] text-text-muted truncate">
                            {item.author} • {formatNumber(item.downloads)}{" "}
                            {language === "tr" ? "indirme" : "downloads"}
                          </span>
                        </div>
                        <p className="text-xs text-text-secondary line-clamp-1 mt-1">
                          {item.description}
                        </p>
                        {item.categories.length > 0 && (
                          <div className="flex flex-wrap gap-1 mt-1.5">
                            {item.categories.slice(0, 5).map((cat) => (
                              <span
                                key={cat}
                                className="px-2 py-0.5 rounded-md bg-bg-elevated text-[10px] text-text-muted capitalize"
                              >
                                {cat}
                              </span>
                            ))}
                          </div>
                        )}
                      </div>
                    </div>

                    <div
                      className="flex items-center gap-2 shrink-0"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <button
                        onClick={() => handleOpenInspector(item)}
                        className="px-3 py-1.5 rounded-xl bg-bg-elevated hover:bg-bg-secondary text-xs font-medium text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition"
                      >
                        <Eye className="w-3.5 h-3.5" />
                        <span>{t.storeInspectBtn}</span>
                      </button>

                      <button
                        disabled={isInstalling || isInstalled}
                        onClick={() =>
                          handleInstallItem(
                            item.projectId,
                            item.provider,
                            item.contentType
                          )
                        }
                        className={`px-3.5 py-1.5 rounded-xl text-xs font-semibold flex items-center gap-1.5 transition ${
                          isInstalled
                            ? "bg-emerald-500/20 text-emerald-300"
                            : "bg-accent-primary hover:opacity-90 text-white disabled:opacity-50"
                        }`}
                      >
                        {isInstalled ? (
                          <>
                            <Check className="w-3.5 h-3.5" />
                            <span>{t.storeInstalledBadge}</span>
                          </>
                        ) : (
                          <>
                            <Download className="w-3.5 h-3.5" />
                            <span>
                              {isInstalling
                                ? t.storeInstallingBtn
                                : t.storeInstallBtn}
                            </span>
                          </>
                        )}
                      </button>
                    </div>
                  </div>
                );
              }

              // Grid Card View
              return (
                <div
                  key={item.provider + "-" + item.projectId}
                  onClick={() => handleOpenInspector(item)}
                  className="rounded-2xl bg-bg-card border border-border-subtle p-4 flex flex-col justify-between space-y-3.5 hover:border-accent-primary/50 cursor-pointer transition"
                >
                  <div className="space-y-2.5">
                    <div className="flex items-start gap-3">
                      {item.iconUrl ? (
                        <img
                          src={item.iconUrl}
                          alt={item.title}
                          className="w-12 h-12 rounded-xl object-cover bg-bg-elevated shrink-0"
                        />
                      ) : (
                        <div className="w-12 h-12 rounded-xl bg-bg-elevated flex items-center justify-center text-accent-secondary shrink-0">
                          <Package className="w-6 h-6" />
                        </div>
                      )}
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center justify-between gap-2">
                          <h3 className="font-semibold text-sm text-text-primary truncate">
                            {item.title}
                          </h3>
                          <span className="px-2 py-0.5 rounded bg-bg-elevated text-[10px] font-medium text-text-secondary uppercase shrink-0">
                            {item.provider}
                          </span>
                        </div>
                        <p className="text-[11px] text-text-muted mt-0.5">
                          {item.author} • {formatNumber(item.downloads)}{" "}
                          {language === "tr" ? "indirme" : "downloads"}
                        </p>
                        <p className="text-xs text-text-secondary line-clamp-2 mt-1.5">
                          {item.description}
                        </p>
                      </div>
                    </div>

                    {item.categories.length > 0 && (
                      <div className="flex flex-wrap gap-1 pt-0.5">
                        {item.categories.slice(0, 4).map((cat) => (
                          <span
                            key={cat}
                            className="px-2 py-0.5 rounded-md bg-bg-elevated text-[10px] text-text-muted capitalize"
                          >
                            {cat}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>

                  <div
                    className="flex items-center justify-end gap-2 pt-1"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <button
                      onClick={() => handleOpenInspector(item)}
                      className="px-3 py-1.5 rounded-xl bg-bg-elevated hover:bg-bg-secondary text-xs font-medium text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition"
                    >
                      <Eye className="w-3.5 h-3.5" />
                      <span>{t.storeInspectBtn}</span>
                    </button>

                    <button
                      disabled={isInstalling || isInstalled}
                      onClick={() =>
                        handleInstallItem(
                          item.projectId,
                          item.provider,
                          item.contentType
                        )
                      }
                      className={`px-3.5 py-1.5 rounded-xl text-xs font-semibold flex items-center gap-1.5 transition ${
                        isInstalled
                          ? "bg-emerald-500/20 text-emerald-300"
                          : "bg-accent-primary hover:opacity-90 text-white disabled:opacity-50"
                      }`}
                    >
                      {isInstalled ? (
                        <>
                          <Check className="w-3.5 h-3.5" />
                          <span>{t.storeInstalledBadge}</span>
                        </>
                      ) : (
                        <>
                          <Download className="w-3.5 h-3.5" />
                          <span>
                            {isInstalling
                              ? t.storeInstallingBtn
                              : t.storeInstallBtn}
                          </span>
                        </>
                      )}
                    </button>
                  </div>
                </div>
              );
            })}
          </div>

          {/* Infinite Scroll Indicator / Load More Button */}
          <div className="py-4 text-center">
            {loadingMore ? (
              <span className="text-xs text-text-secondary animate-pulse">
                {language === "tr"
                  ? "Daha fazla içerik yükleniyor..."
                  : "Loading more content..."}
              </span>
            ) : hasMore ? (
              <button
                onClick={() => {
                  const nextOffset = offset + 24;
                  setOffset(nextOffset);
                  fetchPage(nextOffset, true);
                }}
                className="px-4 py-2 rounded-xl bg-bg-card hover:bg-bg-elevated border border-border-subtle text-xs text-text-secondary hover:text-text-primary transition"
              >
                {language === "tr" ? "Daha Fazla Göster" : "Load More"}
              </button>
            ) : (
              <span className="text-[11px] text-text-muted">
                {language === "tr"
                  ? "Tüm sonuçlar listelendi."
                  : "All results loaded."}
              </span>
            )}
          </div>
        </>
      )}

      {/* Inspector Loading Toast */}
      {inspectorLoading && (
        <div className="fixed bottom-5 right-5 z-50 px-4 py-2.5 rounded-xl bg-bg-card border border-accent-primary/40 shadow-xl text-xs text-text-primary flex items-center gap-2">
          <Sparkles className="w-4 h-4 text-accent-primary animate-spin" />
          <span>
            {language === "tr"
              ? "Proje detayları yükleniyor..."
              : "Loading project details..."}
          </span>
        </div>
      )}

      {/* Detailed In-App Mod / Modpack Inspector Modal */}
      {inspectingProject && (
        <div
          className="fixed inset-0 z-50 bg-black/75 backdrop-blur-sm flex items-center justify-center p-4"
          onClick={() => setInspectingProject(null)}
        >
          <div
            className="w-full max-w-4xl rounded-2xl bg-bg-card border border-border-subtle shadow-2xl overflow-hidden flex flex-col max-h-[88vh]"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Modal Top Header */}
            <div className="p-4 bg-bg-secondary border-b border-border-subtle flex items-start justify-between gap-4">
              <div className="flex items-start gap-3.5 min-w-0">
                {inspectingProject.iconUrl ? (
                  <img
                    src={inspectingProject.iconUrl}
                    alt={inspectingProject.title}
                    onClick={() =>
                      inspectingProject.iconUrl &&
                      setZoomedImageUrl(inspectingProject.iconUrl)
                    }
                    className="w-14 h-14 rounded-xl object-cover bg-bg-elevated cursor-zoom-in border border-border-subtle shrink-0"
                  />
                ) : (
                  <div className="w-14 h-14 rounded-xl bg-bg-elevated flex items-center justify-center text-accent-secondary shrink-0">
                    <Package className="w-7 h-7" />
                  </div>
                )}

                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <h2 className="text-base font-bold text-text-primary">
                      {inspectingProject.title}
                    </h2>
                    <span className="px-2 py-0.5 rounded bg-accent-primary/20 text-accent-secondary text-[10px] font-semibold uppercase">
                      {inspectingProject.provider}
                    </span>
                    {inspectingProject.license && (
                      <span className="px-2 py-0.5 rounded bg-bg-elevated text-text-muted text-[10px] flex items-center gap-1">
                        <ShieldCheck className="w-3 h-3" />
                        <span>{inspectingProject.license}</span>
                      </span>
                    )}
                  </div>

                  <p className="text-xs text-text-secondary mt-1 line-clamp-2">
                    {inspectingProject.description}
                  </p>

                  {/* Stats & Quick Links Bar */}
                  <div className="flex flex-wrap items-center gap-3 mt-2 text-[11px] text-text-muted">
                    <span className="flex items-center gap-1">
                      <Download className="w-3.5 h-3.5 text-accent-secondary" />
                      <strong className="text-text-primary">
                        {formatNumber(inspectingProject.downloads)}
                      </strong>{" "}
                      {language === "tr" ? "indirme" : "downloads"}
                    </span>

                    {inspectingProject.followers > 0 && (
                      <span className="flex items-center gap-1">
                        <Heart className="w-3.5 h-3.5 text-rose-400" />
                        <strong className="text-text-primary">
                          {formatNumber(inspectingProject.followers)}
                        </strong>{" "}
                        {language === "tr" ? "takipçi" : "followers"}
                      </span>
                    )}

                    {inspectingProject.updatedAt && (
                      <span className="flex items-center gap-1">
                        <Calendar className="w-3.5 h-3.5 text-emerald-400" />
                        <span>{inspectingProject.updatedAt.slice(0, 10)}</span>
                      </span>
                    )}

                    {inspectingProject.sourceUrl && (
                      <a
                        href={inspectingProject.sourceUrl}
                        target="_blank"
                        rel="noreferrer"
                        className="flex items-center gap-1 text-accent-secondary hover:underline"
                      >
                        <Code2 className="w-3.5 h-3.5" />
                        <span>Kaynak Kod</span>
                      </a>
                    )}

                    {inspectingProject.wikiUrl && (
                      <a
                        href={inspectingProject.wikiUrl}
                        target="_blank"
                        rel="noreferrer"
                        className="flex items-center gap-1 text-accent-secondary hover:underline"
                      >
                        <BookOpen className="w-3.5 h-3.5" />
                        <span>Wiki</span>
                      </a>
                    )}

                    {inspectingProject.issuesUrl && (
                      <a
                        href={inspectingProject.issuesUrl}
                        target="_blank"
                        rel="noreferrer"
                        className="flex items-center gap-1 text-accent-secondary hover:underline"
                      >
                        <ExternalLink className="w-3.5 h-3.5" />
                        <span>Issues</span>
                      </a>
                    )}
                  </div>
                </div>
              </div>

              <button
                onClick={() => setInspectingProject(null)}
                className="p-1.5 rounded-lg bg-bg-elevated text-text-secondary hover:text-white shrink-0"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            {/* Modal Scrollable Body */}
            <div className="p-5 space-y-5 overflow-y-auto flex-1">
              {/* Categories & Supported Versions/Loaders */}
              {(inspectingProject.categories.length > 0 ||
                inspectingProject.loaders.length > 0 ||
                inspectingProject.gameVersions.length > 0) && (
                <div className="flex flex-wrap items-center gap-2 p-3 rounded-xl bg-bg-secondary/60 border border-border-subtle">
                  {inspectingProject.loaders.map((ldr) => (
                    <span
                      key={ldr}
                      className="px-2.5 py-0.5 rounded-md bg-accent-primary/20 text-accent-secondary text-[11px] font-semibold uppercase"
                    >
                      {ldr}
                    </span>
                  ))}
                  {inspectingProject.categories.map((cat) => (
                    <span
                      key={cat}
                      className="px-2.5 py-0.5 rounded-md bg-bg-elevated text-text-secondary text-[11px] capitalize"
                    >
                      {cat}
                    </span>
                  ))}
                  {inspectingProject.gameVersions.slice(-6).map((gv) => (
                    <span
                      key={gv}
                      className="px-2 py-0.5 rounded bg-emerald-500/15 text-emerald-300 text-[10px] font-mono"
                    >
                      {gv}
                    </span>
                  ))}
                </div>
              )}

              {/* Screenshot Gallery (Left-Click to Zoom) */}
              {inspectingProject.galleryUrls.length > 0 && (
                <div className="space-y-2">
                  <div className="flex items-center justify-between text-xs font-semibold text-text-primary">
                    <span>
                      {language === "tr"
                        ? `Görseller (${inspectingProject.galleryUrls.length}) — Büyütmek için sol tıklayın`
                        : `Screenshots (${inspectingProject.galleryUrls.length}) — Left-click to zoom`}
                    </span>
                  </div>
                  <div className="flex gap-3 overflow-x-auto pb-2">
                    {inspectingProject.galleryUrls.map((url, idx) => (
                      <div
                        key={url + idx}
                        onClick={() => setZoomedImageUrl(url)}
                        className="relative group h-36 rounded-xl overflow-hidden border border-border-subtle shrink-0 cursor-zoom-in bg-bg-primary"
                      >
                        <img
                          src={url}
                          alt={`Screenshot ${idx + 1}`}
                          className="h-full w-auto object-cover group-hover:scale-105 transition duration-200"
                        />
                        <div className="absolute inset-0 bg-black/30 opacity-0 group-hover:opacity-100 transition flex items-center justify-center">
                          <ZoomIn className="w-6 h-6 text-white drop-shadow" />
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Version Selector & Dependency Badges */}
              {inspectingProject.versions.length > 0 && (
                <div className="p-3.5 rounded-xl bg-bg-secondary border border-border-subtle space-y-2.5">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <span className="text-xs font-semibold text-text-primary flex items-center gap-1.5">
                      <Layers className="w-3.5 h-3.5 text-accent-primary" />
                      <span>
                        {language === "tr"
                          ? `Kurulacak Sürüm (${inspectingProject.versions.length} uygun sürüm):`
                          : `Select Version (${inspectingProject.versions.length} available):`}
                      </span>
                    </span>
                    <select
                      value={selectedVersionId}
                      onChange={(e) => setSelectedVersionId(e.target.value)}
                      className="px-3 py-1.5 rounded-lg bg-bg-card border border-border-subtle text-xs text-text-primary max-w-md"
                    >
                      {inspectingProject.versions.map((v) => (
                        <option key={v.versionId} value={v.versionId}>
                          {v.versionNumber} — {v.fileName} (
                          {(v.sizeBytes / (1024 * 1024)).toFixed(2)} MB)
                        </option>
                      ))}
                    </select>
                  </div>

                  {(() => {
                    const activeVer = inspectingProject.versions.find(
                      (v) => v.versionId === selectedVersionId
                    );
                    if (!activeVer) return null;
                    return (
                      <div className="flex flex-wrap items-center gap-1.5 pt-1">
                        {activeVer.loaders.map((l) => (
                          <span
                            key={l}
                            className="px-2 py-0.5 rounded bg-accent-primary/15 text-accent-secondary text-[10px] uppercase font-semibold"
                          >
                            {l}
                          </span>
                        ))}
                        {activeVer.gameVersions.slice(0, 6).map((gv) => (
                          <span
                            key={gv}
                            className="px-2 py-0.5 rounded bg-bg-elevated text-text-secondary text-[10px] font-mono"
                          >
                            MC {gv}
                          </span>
                        ))}
                        {activeVer.dependencies.map((d, idx) => (
                          <span
                            key={d.projectId + idx}
                            className={`px-2 py-0.5 rounded text-[10px] font-medium ${
                              d.dependencyType === "required"
                                ? "bg-amber-500/20 text-amber-300"
                                : "bg-bg-elevated text-text-muted"
                            }`}
                          >
                            {d.dependencyType === "required"
                              ? language === "tr"
                                ? "Zorunlu Bağımlılık (Otomatik Kurulur): "
                                : "Required Dependency: "
                              : language === "tr"
                              ? "İsteğe Bağlı: "
                              : "Optional: "}
                            {d.projectId}
                          </span>
                        ))}
                      </div>
                    );
                  })()}
                </div>
              )}

              {/* Rich Rendered Mod Description (Modrinth Markdown + HTML) */}
              <div className="space-y-2">
                <h3 className="text-xs font-bold text-text-primary uppercase tracking-wider">
                  {language === "tr"
                    ? "Detaylı Açıklama (Görsellere sol tıklayarak büyütebilirsiniz)"
                    : "Detailed Description (Left-click any image to zoom)"}
                </h3>
                <div
                  onClick={handleDescriptionClick}
                  className="p-4 rounded-xl bg-bg-primary border border-border-subtle text-xs text-text-secondary leading-relaxed overflow-x-hidden break-words space-y-1"
                  dangerouslySetInnerHTML={{
                    __html: renderMixedMarkdownAndHtml(
                      inspectingProject.body || inspectingProject.description
                    ),
                  }}
                />
              </div>
            </div>

            {/* Modal Bottom Actions */}
            <div className="p-3.5 bg-bg-secondary border-t border-border-subtle flex items-center justify-between gap-2">
              <span className="text-[11px] text-text-muted">
                {language === "tr"
                  ? "İpucu: Görsellerin üzerine sol tıklayarak tam ekran inceleyebilirsiniz."
                  : "Tip: Left-click any image to inspect in full screen."}
              </span>
              <div className="flex items-center gap-2">
                <button
                  onClick={() => setInspectingProject(null)}
                  className="px-4 py-1.5 rounded-xl bg-bg-elevated text-xs font-medium text-text-secondary hover:text-white"
                >
                  {t.closeModal}
                </button>
                <button
                  onClick={() => {
                    handleInstallItem(
                      inspectingProject.projectId,
                      inspectingProject.provider,
                      inspectingProject.contentType,
                      selectedVersionId
                    );
                    setInspectingProject(null);
                  }}
                  className="px-4 py-1.5 rounded-xl bg-accent-primary hover:opacity-90 text-white text-xs font-semibold flex items-center gap-1.5"
                >
                  <Download className="w-3.5 h-3.5" />
                  <span>{t.storeInstallBtn}</span>
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Full-Screen Image Lightbox Modal (Left-Click Zoom) */}
      {zoomedImageUrl && (
        <div
          className="fixed inset-0 z-[70] bg-black/90 backdrop-blur-md flex flex-col items-center justify-center p-6 cursor-zoom-out"
          onClick={() => setZoomedImageUrl(null)}
        >
          <div className="absolute top-4 right-4 flex items-center gap-2">
            <span className="px-3 py-1 rounded-lg bg-white/10 text-white text-xs flex items-center gap-1.5">
              <Maximize2 className="w-3.5 h-3.5" />
              <span>
                {language === "tr"
                  ? "Kapatmak için herhangi bir yere tıklayın"
                  : "Click anywhere to close"}
              </span>
            </span>
            <button
              onClick={() => setZoomedImageUrl(null)}
              className="p-2 rounded-xl bg-white/15 hover:bg-white/25 text-white transition"
            >
              <X className="w-5 h-5" />
            </button>
          </div>

          <img
            src={zoomedImageUrl}
            alt="Zoomed Preview"
            className="max-w-[92vw] max-h-[86vh] rounded-2xl object-contain shadow-2xl border border-white/15"
            onClick={(e) => e.stopPropagation()}
          />
        </div>
      )}
    </div>
  );
};
