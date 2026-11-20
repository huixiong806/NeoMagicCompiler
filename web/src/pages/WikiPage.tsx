import { useEffect, useState } from 'react';
import { BookOpen } from 'lucide-react';
import { Link, useParams } from 'react-router-dom';
import { SimpleMarkdown } from '../components/level/SimpleMarkdown';
import { defaultEngineClient } from '../engine/OnlineWasmClient';
import type { WikiEntry } from '../engine/types';

export function WikiPage() {
  const { topic } = useParams();
  const [entries, setEntries] = useState<WikiEntry[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    defaultEngineClient
      .listWiki()
      .then((list) => {
        if (active) setEntries(list);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const current = entries.find((e) => e.topic === topic) ?? entries[0];

  return (
    <div className="grid gap-5 lg:grid-cols-12">
      <aside className="rounded-2xl border border-slate-800 bg-slate-900/70 p-4 lg:col-span-3">
        <div className="mb-3 flex items-center gap-2 text-sm font-bold text-white">
          <BookOpen className="h-4 w-4 text-cyan-400" /> 概念图鉴目录
        </div>
        <div className="space-y-1">
          {entries.map((item) => (
            <Link
              key={item.topic}
              to={`/wiki/${item.topic}`}
              className={`block rounded-lg px-3 py-2 text-xs font-medium transition ${
                current?.topic === item.topic
                  ? 'bg-cyan-500/20 text-cyan-200 ring-1 ring-cyan-500/40'
                  : 'text-slate-300 hover:bg-slate-800'
              }`}
            >
              {item.title}
            </Link>
          ))}
        </div>
      </aside>

      <article className="rounded-2xl border border-slate-800 bg-slate-900/70 p-6 lg:col-span-9">
        {loading ? (
          <div className="text-xs text-slate-400">正在加载概念图鉴…</div>
        ) : current ? (
          <SimpleMarkdown content={current.markdown} />
        ) : (
          <div className="text-xs text-slate-400">暂无词条。</div>
        )}
      </article>
    </div>
  );
}
