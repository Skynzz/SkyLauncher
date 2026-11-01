import { invoke } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  Check,
  CheckCircle2,
  Copy,
  Wrench,
  X,
} from "lucide-react";
import React, { useState } from "react";
import type {
  CrashAnalysisReport,
  InstanceConfig,
  RecoveryAction,
} from "../hooks/useLauncher";
import { Language, translations } from "../i18n/translations";

interface CrashReportModalProps {
  report: CrashAnalysisReport;
  onClose: () => void;
  onRecovered: (updatedInstance: InstanceConfig, relaunch: boolean) => void;
  language: Language;
}

export const CrashReportModal: React.FC<CrashReportModalProps> = ({
  report,
  onClose,
  onRecovered,
  language,
}) => {
  const t = translations[language];
  const [applyingAction, setApplyingAction] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const handleApplyRecovery = async (action: RecoveryAction) => {
    setApplyingAction(action.actionType);
    try {
      const updated = await invoke<InstanceConfig>("apply_crash_recovery", {
        instanceId: report.instanceId,
        actionType: action.actionType,
        targetValue: action.targetValue,
      });
      onRecovered(updated, action.actionType === "disable_mod");
    } catch {
      setApplyingAction(null);
    }
  };

  const handleCopyError = async () => {
    const title = language === "tr" ? report.titleTr : report.titleEn;
    const summary = language === "tr" ? report.summaryTr : report.summaryEn;
    const fullText = [
      `[${title}] (Exit Code: ${report.exitCode})`,
      summary,
      "",
      ...report.relevantLogLines,
    ].join("\n");

    try {
      await navigator.clipboard.writeText(fullText);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // ignore clipboard errors
    }
  };

  return (
    <div className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4">
      <div className="w-full max-w-lg rounded-2xl bg-bg-card border border-border-subtle shadow-2xl overflow-hidden flex flex-col max-h-[85vh]">
        <div className="p-4 border-b border-border-subtle flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            <AlertTriangle className="w-5 h-5 text-amber-400" />
            <h2 className="text-sm font-bold text-text-primary">
              {language === "tr" ? report.titleTr : report.titleEn}
            </h2>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg bg-bg-elevated text-text-secondary"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="p-4 space-y-4 overflow-y-auto">
          <p className="text-xs text-text-secondary leading-relaxed">
            {language === "tr" ? report.summaryTr : report.summaryEn}
          </p>

          {report.recoveryActions.length > 0 && (
            <div className="p-3.5 rounded-xl bg-bg-secondary border border-border-subtle space-y-2.5">
              <div className="text-xs font-semibold text-text-primary flex items-center gap-1.5">
                <Wrench className="w-3.5 h-3.5 text-accent-secondary" />
                <span>{t.oneClickRecoveryTitle}</span>
              </div>
              <div className="flex flex-wrap gap-2">
                {report.recoveryActions.map((act) => (
                  <button
                    key={act.actionType + act.targetValue}
                    disabled={applyingAction !== null}
                    onClick={() => handleApplyRecovery(act)}
                    className="px-3.5 py-2 rounded-xl bg-accent-primary hover:opacity-90 disabled:opacity-50 text-white text-xs font-semibold flex items-center gap-1.5 transition"
                  >
                    <CheckCircle2 className="w-3.5 h-3.5" />
                    <span>
                      {language === "tr" ? act.labelTr : act.labelEn}
                    </span>
                  </button>
                ))}
              </div>
            </div>
          )}

          {report.relevantLogLines.length > 0 && (
            <div className="space-y-1.5">
              <div className="flex items-center justify-between">
                <div className="text-[11px] font-medium text-text-muted">
                  {t.relevantLogsTitle}
                </div>
                <button
                  type="button"
                  onClick={handleCopyError}
                  className="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-bg-elevated hover:bg-bg-secondary text-[11px] font-medium text-text-secondary hover:text-text-primary transition"
                >
                  {copied ? (
                    <>
                      <Check className="w-3 h-3 text-emerald-400" />
                      <span className="text-emerald-400">{t.copiedBtn}</span>
                    </>
                  ) : (
                    <>
                      <Copy className="w-3 h-3" />
                      <span>{t.copyBtn}</span>
                    </>
                  )}
                </button>
              </div>
              <pre className="p-3 rounded-xl bg-bg-primary border border-border-subtle text-[11px] font-mono text-text-secondary overflow-x-auto max-h-40 whitespace-pre-wrap">
                {report.relevantLogLines.join("\n")}
              </pre>
            </div>
          )}
        </div>

        <div className="p-3 border-t border-border-subtle flex items-center justify-between">
          <button
            type="button"
            onClick={handleCopyError}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-bg-elevated hover:bg-bg-secondary text-xs font-medium text-text-secondary hover:text-text-primary transition"
          >
            {copied ? (
              <>
                <Check className="w-3.5 h-3.5 text-emerald-400" />
                <span className="text-emerald-400">{t.copiedBtn}</span>
              </>
            ) : (
              <>
                <Copy className="w-3.5 h-3.5" />
                <span>{t.copyBtn}</span>
              </>
            )}
          </button>

          <button
            onClick={onClose}
            className="px-4 py-1.5 rounded-xl bg-bg-elevated text-xs font-medium text-text-primary"
          >
            {t.closeModal}
          </button>
        </div>
      </div>
    </div>
  );
};
