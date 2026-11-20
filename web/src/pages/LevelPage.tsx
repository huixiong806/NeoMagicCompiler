import { useEffect, useRef, useState } from 'react';
import { ArrowLeft } from 'lucide-react';
import { Link, useParams } from 'react-router-dom';
import { useAppContext } from '../app/AppShell';
import { GoalCard } from '../components/level/GoalCard';
import { InfoDrawer } from '../components/level/InfoDrawer';
import { WikiDrawer } from '../components/level/WikiDrawer';
import type { CodeEditorHandle } from '../components/playground/CodeEditor';
import { PlaygroundWorkspace } from '../components/playground/PlaygroundWorkspace';
import { defaultEngineClient } from '../engine/OnlineWasmClient';
import type { LevelPayload, LevelSummary, SubmitDto } from '../engine/types';
import { renderSandbox } from '../sandboxes/registry';

export function LevelPage() {
  const { id = '1-1' } = useParams();
  const { progress, refreshProgress } = useAppContext();
  const editorRef = useRef<CodeEditorHandle | null>(null);

  const [level, setLevel] = useState<LevelPayload | null>(null);
  const [allLevels, setAllLevels] = useState<LevelSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [drawerMode, setDrawerMode] = useState<'lesson' | 'hints' | null>(null);
  const [wikiTopic, setWikiTopic] = useState<string | null>(null);
  const [justPassed, setJustPassed] = useState(false);

  // 仅在切换关卡 id 时重新加载关卡内容并重置通关态，避免提交更新 progress 时卸载工作台
  useEffect(() => {
    let active = true;
    setLoading(true);
    setJustPassed(false);
    setDrawerMode(null);
    defaultEngineClient
      .loadLevel(id)
      .then((payload) => {
        if (!active) return;
        setLevel(payload);
        setError(null);
      })
      .catch((err) => {
        if (active) setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [id]);

  // 静默刷新关卡摘要列表（用于通关后展示解锁的下一主线与支线按钮）
  useEffect(() => {
    let active = true;
    defaultEngineClient
      .listLevels(progress)
      .then((summaries) => {
        if (active) setAllLevels(summaries);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [progress]);

  const handleSubmitted = (res: SubmitDto) => {
    refreshProgress();
    if (res.passed) {
      setJustPassed(true);
    }
  };

  if (loading) {
    return <div className="py-16 text-center text-sm text-slate-400">正在加载关卡 {id}…</div>;
  }
  if (error || !level) {
    return (
      <div className="space-y-3 rounded-2xl border border-rose-500/40 bg-rose-950/30 p-6">
        <div className="text-sm font-semibold text-rose-200">无法加载关卡 {id}</div>
        <div className="font-mono text-xs text-rose-300">{error}</div>
        <Link to="/" className="inline-flex items-center gap-1 text-xs text-cyan-300 hover:underline">
          <ArrowLeft className="h-3.5 w-3.5" /> 返回技能树
        </Link>
      </div>
    );
  }

  const passed = Boolean(progress.levels[level.id]?.passed);
  const nextUnlocked = allLevels.filter(
    (l) => l.prerequisites.includes(level.id) && l.id !== level.id,
  );
  const nextMain = nextUnlocked.find((l) => l.kind !== 'challenge');
  const nextSide = nextUnlocked.find((l) => l.kind === 'challenge');

  return (
    <div className="space-y-3">
      <GoalCard
        meta={level.meta}
        passed={passed}
        justPassed={justPassed}
        nextMain={nextMain}
        nextSide={nextSide}
        onOpenLesson={() => setDrawerMode('lesson')}
        onOpenHints={() => setDrawerMode('hints')}
        onOpenWiki={(topic) => setWikiTopic(topic)}
      />

      {/* 纯净双栏：左图形沙盘 + 右代码工作台 */}
      <div className="grid gap-3.5 lg:grid-cols-12">
        <div className="lg:col-span-5">
          {renderSandbox({
            config: level.meta.sandbox,
            onFocusEditor: () => editorRef.current?.focusEditableLine(),
          })}
        </div>

        <div className="lg:col-span-7">
          <PlaygroundWorkspace
            level={level}
            editorRef={editorRef}
            onSubmitted={handleSubmitted}
          />
        </div>
      </div>

      <InfoDrawer
        mode={drawerMode}
        title={`${level.id} · ${level.meta.title}`}
        beforeText={level.meta.goalSummary.before}
        afterText={level.meta.goalSummary.after}
        lessonMarkdown={level.lessonMarkdown}
        hints={level.meta.hints}
        wikiRefs={level.meta.wikiRefs}
        onSwitchMode={setDrawerMode}
        onOpenWiki={(topic) => {
          setDrawerMode(null);
          setWikiTopic(topic);
        }}
        onClose={() => setDrawerMode(null)}
      />

      <WikiDrawer topic={wikiTopic} onClose={() => setWikiTopic(null)} />
    </div>
  );
}
