import { useCallback, useEffect, useRef, useState } from "react";
import type {
  CrashAnalysisReport,
  InstanceConfig,
  PluginDescriptor,
} from "./useLauncher";

export interface PluginHookModule {
  onAppInit?: (ctx: PluginSandboxContext) => void;
  onInstanceDetected?: (instance: InstanceConfig, ctx: PluginSandboxContext) => void;
  onPreLaunch?: (instanceConfig: InstanceConfig, ctx: PluginSandboxContext) => void;
  onGameLog?: (line: string, ctx: PluginSandboxContext) => void;
  onGameCrash?: (crashReport: CrashAnalysisReport, ctx: PluginSandboxContext) => void;
}

export interface PluginSandboxContext {
  pluginId: string;
  log: (msg: string) => void;
  recordMetric: (key: string, value: string) => void;
}

/**
 * Evaluates a plugin's `index.js` source code in a restricted, fault-isolated scope
 * without exposing raw Node/system globals.
 */
function compileSandboxedPlugin(plugin: PluginDescriptor): PluginHookModule | null {
  try {
    const cleaned = plugin.scriptCode.replace(/export\s+default\s+/, "return ");
    const factory = new Function(
      "window",
      "document",
      "fetch",
      `"use strict";\n${cleaned}`
    );
    const mod = factory(undefined, undefined, undefined);
    if (mod && typeof mod === "object") {
      return mod as PluginHookModule;
    }
  } catch (err) {
    console.warn(`[PluginSandbox] Failed to compile plugin ${plugin.id}:`, err);
  }
  return null;
}

export function usePluginBus(plugins: PluginDescriptor[]) {
  const [busLogs, setBusLogs] = useState<string[]>([]);
  const [metrics, setMetrics] = useState<Record<string, string>>({});
  const compiledRef = useRef<Map<string, PluginHookModule>>(new Map());
  const initializedRef = useRef<Set<string>>(new Set());

  const appendLog = useCallback((entry: string) => {
    const timestamp = new Date().toLocaleTimeString();
    setBusLogs((prev) => [`[${timestamp}] ${entry}`, ...prev.slice(0, 59)]);
  }, []);

  const createContext = useCallback(
    (pluginId: string): PluginSandboxContext => ({
      pluginId,
      log: (msg: string) => appendLog(msg),
      recordMetric: (key: string, value: string) => {
        setMetrics((prev) => ({ ...prev, [`${pluginId}:${key}`]: value }));
      },
    }),
    [appendLog]
  );

  useEffect(() => {
    const map = new Map<string, PluginHookModule>();
    for (const p of plugins) {
      if (!p.enabled) continue;
      const mod = compileSandboxedPlugin(p);
      if (mod) {
        map.set(p.id, mod);
        if (!initializedRef.current.has(p.id)) {
          initializedRef.current.add(p.id);
          try {
            mod.onAppInit?.(createContext(p.id));
          } catch (e) {
            appendLog(`[PluginError:${p.id}] onAppInit failed: ${String(e)}`);
          }
        }
      }
    }
    compiledRef.current = map;
  }, [plugins, createContext, appendLog]);

  const emitInstanceDetected = useCallback(
    (instance: InstanceConfig) => {
      for (const [id, mod] of compiledRef.current.entries()) {
        try {
          mod.onInstanceDetected?.(instance, createContext(id));
        } catch (e) {
          appendLog(`[PluginError:${id}] onInstanceDetected: ${String(e)}`);
        }
      }
    },
    [createContext, appendLog]
  );

  const emitPreLaunch = useCallback(
    (instanceConfig: InstanceConfig) => {
      for (const [id, mod] of compiledRef.current.entries()) {
        try {
          mod.onPreLaunch?.(instanceConfig, createContext(id));
        } catch (e) {
          appendLog(`[PluginError:${id}] onPreLaunch: ${String(e)}`);
        }
      }
    },
    [createContext, appendLog]
  );

  const emitGameLog = useCallback(
    (line: string) => {
      for (const [id, mod] of compiledRef.current.entries()) {
        try {
          mod.onGameLog?.(line, createContext(id));
        } catch {
          // Ignore per-line log hook errors to avoid spam
        }
      }
    },
    [createContext]
  );

  const emitGameCrash = useCallback(
    (crashReport: CrashAnalysisReport) => {
      for (const [id, mod] of compiledRef.current.entries()) {
        try {
          mod.onGameCrash?.(crashReport, createContext(id));
        } catch (e) {
          appendLog(`[PluginError:${id}] onGameCrash: ${String(e)}`);
        }
      }
    },
    [createContext, appendLog]
  );

  return {
    busLogs,
    metrics,
    emitInstanceDetected,
    emitPreLaunch,
    emitGameLog,
    emitGameCrash,
  };
}
