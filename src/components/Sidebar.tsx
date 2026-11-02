import { invoke } from "@tauri-apps/api/core";
import {
  Gamepad2,
  Globe,
  Minimize2,
  Plus,
  Settings as SettingsIcon,
  ShoppingBag,
  Square,
  Trash2,
  User,
} from "lucide-react";
import React, { useState } from "react";
import type { AuthStore, SystemTelemetry } from "../hooks/useLauncher";
import { Language, translations } from "../i18n/translations";

interface SidebarProps {
  activeTab: "instances" | "store" | "downloader" | "settings";
  setActiveTab: (
    tab: "instances" | "store" | "downloader" | "settings"
  ) => void;
  authStore: AuthStore;
  setAuthStore: React.Dispatch<React.SetStateAction<AuthStore>>;
  telemetry: SystemTelemetry;
  language: Language;
  onToggleLanguage: (lang: Language) => void;
  windowSizeMode?: "small" | "medium";
  onToggleWindowSizeMode: (mode: "small" | "medium") => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  activeTab,
  setActiveTab,
  authStore,
  setAuthStore,
  language,
  onToggleLanguage,
  windowSizeMode,
  onToggleWindowSizeMode,
}) => {
  const t = translations[language];
  const [newUsername, setNewUsername] = useState("");
  const [showAddInput, setShowAddInput] = useState(
    authStore.profiles.length === 0
  );

  const activeProfile =
    authStore.profiles.find((p) => p.id === authStore.activeProfileId) ||
    authStore.profiles[0] ||
    null;

  const handleCreateProfile = async (e: React.FormEvent) => {
    e.preventDefault();
    const clean = newUsername.trim();
    if (!clean) return;
    try {
      const updated = await invoke<AuthStore>("create_offline_profile", {
        username: clean,
        avatarColor: "#0ea5e9",
      });
      setAuthStore(updated);
      setNewUsername("");
      setShowAddInput(false);
    } catch {
      // ignore invalid username
    }
  };

  const handleSelectProfile = async (profileId: string) => {
    try {
      const updated = await invoke<AuthStore>("set_active_profile", {
        profileId,
      });
      setAuthStore(updated);
    } catch {
      // ignore
    }
  };

  const handleDeleteProfile = async (profileId: string) => {
    try {
      const updated = await invoke<AuthStore>("delete_offline_profile", {
        profileId,
      });
      setAuthStore(updated);
    } catch {
      // ignore
    }
  };

  return (
    <aside className="w-64 h-screen bg-bg-secondary border-r border-border-subtle flex flex-col justify-between p-4 shrink-0">
      <div className="space-y-6">
        {/* Clean Brand Header */}
        <div className="flex items-center justify-between px-1 pt-1">
          <div className="flex items-center gap-2.5">
            <div className="w-8 h-8 rounded-lg bg-accent-primary flex items-center justify-center text-white font-bold text-sm shadow-sm">
              S
            </div>
            <span className="font-bold text-base tracking-tight text-text-primary">
              SkyLauncher
            </span>
          </div>

          <button
            onClick={() => onToggleLanguage(language === "tr" ? "en" : "tr")}
            className="flex items-center gap-1 px-2 py-1 rounded-md bg-bg-elevated hover:bg-bg-card text-text-secondary hover:text-text-primary text-xs font-medium transition border border-border-subtle"
            title={language === "tr" ? "Dili değiştir (EN)" : "Change language (TR)"}
          >
            <Globe className="w-3.5 h-3.5" />
            <span>{language.toUpperCase()}</span>
          </button>
        </div>

        {/* Simple Navigation */}
        <nav className="space-y-1">
          <button
            onClick={() => setActiveTab("instances")}
            className={`w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-sm font-medium transition ${
              activeTab === "instances" || activeTab === "downloader"
                ? "bg-accent-primary text-white"
                : "text-text-secondary hover:bg-bg-elevated hover:text-text-primary"
            }`}
          >
            <Gamepad2 className="w-4 h-4" />
            <span>{t.navInstances}</span>
          </button>

          <button
            onClick={() => setActiveTab("store")}
            className={`w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-sm font-medium transition ${
              activeTab === "store"
                ? "bg-accent-primary text-white"
                : "text-text-secondary hover:bg-bg-elevated hover:text-text-primary"
            }`}
          >
            <ShoppingBag className="w-4 h-4" />
            <span>{t.navStore}</span>
          </button>

          <button
            onClick={() => setActiveTab("settings")}
            className={`w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-sm font-medium transition ${
              activeTab === "settings"
                ? "bg-accent-primary text-white"
                : "text-text-secondary hover:bg-bg-elevated hover:text-text-primary"
            }`}
          >
            <SettingsIcon className="w-4 h-4" />
            <span>{t.navSettings}</span>
          </button>
        </nav>
      </div>

      {/* Bottom Controls: Discrete Window Size & Player Switcher */}
      <div className="space-y-2">
        {/* Discrete 2-Size Window Mode Toggle */}
        <div className="flex items-center justify-between px-2.5 py-1.5 rounded-xl bg-bg-card border border-border-subtle text-xs">
          <span className="text-text-muted text-[11px] font-medium">
            {t.windowSizeModeLabel}
          </span>
          <div className="flex items-center gap-1 bg-bg-elevated rounded-lg p-0.5 border border-border-subtle">
            <button
              type="button"
              onClick={() => onToggleWindowSizeMode("small")}
              className={`px-2 py-0.5 rounded text-[11px] font-medium transition ${
                (windowSizeMode || "medium") === "small"
                  ? "bg-accent-primary text-white"
                  : "text-text-muted hover:text-text-primary"
              }`}
              title={t.windowSizeSmall}
            >
              Küçük
            </button>
            <button
              type="button"
              onClick={() => onToggleWindowSizeMode("medium")}
              className={`px-2 py-0.5 rounded text-[11px] font-medium transition ${
                (windowSizeMode || "medium") === "medium"
                  ? "bg-accent-primary text-white"
                  : "text-text-muted hover:text-text-primary"
              }`}
              title={t.windowSizeMedium}
            >
              Orta
            </button>
          </div>
        </div>

        {/* Compact Player Switcher */}
        <div className="p-3 rounded-xl bg-bg-card border border-border-subtle space-y-2.5">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2 min-w-0">
              <div className="w-7 h-7 rounded-lg bg-accent-primary/20 text-accent-secondary flex items-center justify-center font-bold text-xs shrink-0">
                <User className="w-3.5 h-3.5" />
              </div>
              <div className="min-w-0">
                <div className="text-[11px] text-text-muted leading-none">
                  {t.activeProfile}
                </div>
                <div className="text-sm font-semibold text-text-primary truncate mt-0.5">
                  {activeProfile?.username ||
                    (language === "tr" ? "Oyuncu Belirleyin" : "Set Username")}
                </div>
              </div>
            </div>

            <button
              onClick={() => setShowAddInput(!showAddInput)}
              className="p-1.5 rounded-lg bg-bg-elevated hover:bg-accent-primary hover:text-white text-text-secondary transition"
              title={t.addProfile}
            >
              <Plus className="w-3.5 h-3.5" />
            </button>
          </div>

          {authStore.profiles.length > 1 && (
            <div className="space-y-1 max-h-28 overflow-y-auto pt-1 border-t border-border-subtle">
              {authStore.profiles.map((prof) => {
                const isSelected = prof.id === activeProfile?.id;
                return (
                  <div
                    key={prof.id}
                    className={`flex items-center justify-between px-2 py-1 rounded-lg text-xs transition ${
                      isSelected
                        ? "bg-accent-primary/15 text-accent-secondary font-medium"
                        : "text-text-secondary hover:bg-bg-elevated"
                    }`}
                  >
                    <button
                      onClick={() => handleSelectProfile(prof.id)}
                      className="flex-1 text-left truncate"
                    >
                      {prof.username}
                    </button>
                    <button
                      onClick={() => handleDeleteProfile(prof.id)}
                      className="text-text-muted hover:text-red-400 p-0.5"
                    >
                      <Trash2 className="w-3 h-3" />
                    </button>
                  </div>
                );
              })}
            </div>
          )}

          {showAddInput && (
            <form onSubmit={handleCreateProfile} className="flex gap-1.5 pt-1">
              <input
                type="text"
                maxLength={16}
                value={newUsername}
                onChange={(e) => setNewUsername(e.target.value)}
                placeholder={t.newProfilePlaceholder}
                className="flex-1 min-w-0 px-2.5 py-1.5 rounded-lg bg-bg-primary border border-border-subtle text-xs text-text-primary focus:outline-none focus:border-accent-primary"
              />
              <button
                type="submit"
                className="px-2.5 py-1.5 rounded-lg bg-accent-primary text-white text-xs font-medium shrink-0"
              >
                {t.addProfile}
              </button>
            </form>
          )}
        </div>
      </div>
    </aside>
  );
};
