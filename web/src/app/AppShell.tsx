import { createContext, useContext, useState } from 'react';
import { BookOpen, Cpu, Compass, Map, TerminalSquare } from 'lucide-react';
import { Link, NavLink, Outlet } from 'react-router-dom';
import { loadProgress, updateSettings } from '../engine/progressStorage';
import type { UserProgress } from '../engine/types';

interface AppContextValue {
  progress: UserProgress;
  refreshProgress: () => void;
  toggleFreeExplore: () => void;
}

const AppContext = createContext<AppContextValue>({
  progress: loadProgress(),
  refreshProgress: () => undefined,
  toggleFreeExplore: () => undefined,
});

export function useAppContext(): AppContextValue {
  return useContext(AppContext);
}

export function AppShell() {
  const [progress, setProgress] = useState<UserProgress>(() => loadProgress());

  const refreshProgress = () => setProgress(loadProgress());

  const toggleFreeExplore = () => {
    const next = updateSettings({ freeExplore: !progress.settings.freeExplore });
    setProgress(next);
  };

  const navClass = ({ isActive }: { isActive: boolean }) =>
    `inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-semibold transition ${
      isActive
        ? 'bg-cyan-500/20 text-cyan-200 ring-1 ring-cyan-500/40'
        : 'text-slate-300 hover:bg-slate-800 hover:text-white'
    }`;

  return (
    <AppContext.Provider value={{ progress, refreshProgress, toggleFreeExplore }}>
      <div className="flex min-h-screen flex-col bg-slate-950 text-slate-100">
        <header className="sticky top-0 z-30 border-b border-slate-800/90 bg-slate-950/90 backdrop-blur">
          <div className="mx-auto flex max-w-[1440px] flex-wrap items-center justify-between gap-3 px-4 py-2.5">
            <div className="flex items-center gap-4">
              <Link to="/" className="flex items-center gap-2 text-sm font-bold text-white">
                <span className="flex h-7 w-7 items-center justify-center rounded-lg bg-cyan-500 text-slate-950">
                  <Cpu className="h-4 w-4" />
                </span>
                <span>Neo Magic Compiler</span>
              </Link>

              <nav className="flex items-center gap-1">
                <NavLink to="/" end className={navClass}>
                  <Map className="h-3.5 w-3.5" /> 关卡技能树
                </NavLink>
                <NavLink to="/playground" className={navClass}>
                  <TerminalSquare className="h-3.5 w-3.5" /> 独立 Playground
                </NavLink>
                <NavLink to="/wiki/rv32-registers" className={navClass}>
                  <BookOpen className="h-3.5 w-3.5" /> 概念图鉴
                </NavLink>
              </nav>
            </div>

            <div className="flex items-center gap-2">
              <span
                title="浏览器内置 nmc-wasm 引擎就绪"
                className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/35 bg-emerald-950/30 px-2.5 py-1 font-mono text-[11px] text-emerald-300"
              >
                <span className="h-2 w-2 rounded-full bg-emerald-400" />
                WASM
              </span>

              <button
                type="button"
                onClick={toggleFreeExplore}
                title="开启后无需前置即可跳关"
                className={`inline-flex items-center gap-1 rounded-full border px-2.5 py-1 text-[11px] font-medium transition ${
                  progress.settings.freeExplore
                    ? 'border-amber-400/60 bg-amber-500/20 text-amber-200'
                    : 'border-slate-800 bg-slate-900 text-slate-400 hover:text-slate-200'
                }`}
              >
                <Compass className="h-3.5 w-3.5" />
                <span>自由探索</span>
              </button>
            </div>
          </div>
        </header>

        <main className="mx-auto w-full max-w-[1440px] flex-1 px-4 py-5">
          <Outlet />
        </main>
      </div>
    </AppContext.Provider>
  );
}
