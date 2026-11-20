import { useEffect, useState } from 'react';
import { BookOpen, X } from 'lucide-react';
import { defaultEngineClient } from '../../engine/OnlineWasmClient';
import type { WikiEntry } from '../../engine/types';
import { SimpleMarkdown } from './SimpleMarkdown';

interface WikiDrawerProps {
  topic: string | null;
  onClose: () => void;
}

export function WikiDrawer({ topic, onClose }: WikiDrawerProps) {
  const [entry, setEntry] = useState<WikiEntry | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!topic) return;
    let active = true;
    setEntry(null);
    setError(null);
    defaultEngineClient
      .loadWiki(topic)
      .then((res) => {
        if (active) setEntry(res);
      })
      .catch((err) => {
        if (active) setError(err instanceof Error ? err.message : String(err));
      });
    return () => {
      active = false;
    };
  }, [topic]);

  if (!topic) return null;

  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-slate-950/60 backdrop-blur-xs">
      <div className="flex h-full w-full max-w-lg flex-col border-l border-slate-800 bg-slate-900 shadow-2xl">
        <div className="flex items-center justify-between border-b border-slate-800 px-4 py-3">
          <span className="inline-flex items-center gap-2 text-sm font-bold text-white">
            <BookOpen className="h-4 w-4 text-cyan-400" />
            {entry?.title ?? '概念图鉴'}
          </span>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg p-1 text-slate-400 hover:bg-slate-800 hover:text-white"
          >
            <X className="h-4 w-4" />
          </button>
        </div>
        <div className="flex-1 overflow-y-auto p-4">
          {error ? (
            <div className="text-xs text-rose-300">{error}</div>
          ) : entry ? (
            <SimpleMarkdown content={entry.markdown} />
          ) : (
            <div className="text-xs text-slate-400">正在加载词条…</div>
          )}
        </div>
      </div>
    </div>
  );
}
