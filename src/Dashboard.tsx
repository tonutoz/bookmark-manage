import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Icon } from "./Icon";
import { ThumbBox } from "./Thumbnail";
import { Bookmark, Filter, SidebarCounts, hostOf } from "./types";
import { t, errorText, locale } from "./i18n";

type DashboardData = {
  total: number;
  sites: number;
  tags: number;
  recent: number;
  previous: number;
  unopened: number;
  untagged: number;
  unrated: number;
  noThumb: number;
  stale: number;
  singletons: number;
  /** [0] 은 별점 없음, [1]~[5] 는 그 별점 */
  ratings: number[];
  top: Bookmark[];
  months: { month: string; count: number }[];
};

type BackupInfo = { dir: string | null; lastBackup: number | null };

const n = (value: number) => value.toLocaleString();

/** 가장 긴 막대가 100% 가 되도록 한 비율. 0 이 아니면 최소한 보이게 한다. */
const share = (value: number, max: number) => (value <= 0 || max <= 0 ? "0%" : `${Math.max(3, Math.round((value / max) * 100))}%`);

/** 이번 달을 끝으로 하는 최근 12개월의 "YYYY-MM" 목록. */
function lastMonths() {
  const today = new Date();
  return Array.from({ length: 12 }, (_, i) => {
    const d = new Date(today.getFullYear(), today.getMonth() - (11 - i), 1);
    return { key: `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`, label: d.toLocaleDateString(locale, { month: "short" }) };
  });
}

/**
 * 가져오기로 한꺼번에 들어온 달이 있으면 그 막대가 나머지를 눌러버린다.
 * 가장 큰 값이 두 번째보다 3배 넘게 크면 두 번째 기준으로 높이를 잘라서 그린다.
 */
function chartCap(counts: number[]) {
  const sorted = [...counts].sort((a, b) => b - a);
  const [largest, second] = sorted;
  return second > 0 && largest > second * 3 ? Math.ceil(second * 1.25) : largest;
}

function Bars({
  title,
  rows,
  color,
}: {
  title: string;
  rows: { label: string; count: number; onClick?: () => void }[];
  color?: string;
}) {
  const max = Math.max(0, ...rows.map((r) => r.count));
  return (
    <section className="dash-card">
      <h2>{title}</h2>
      {rows.length === 0 && <p className="hint">{t("아직 보여줄 것이 없습니다.")}</p>}
      {rows.map((r) => {
        const content = (
          <>
            <span className="bar-label ellipsis">{r.label}</span>
            <span className="bar-track">
              <span className="bar-fill" style={{ width: share(r.count, max), background: color }} />
            </span>
            <span className="bar-count mono">{n(r.count)}</span>
          </>
        );
        return r.onClick ? (
          <button key={r.label} type="button" className="bar-row" onClick={r.onClick}>
            {content}
          </button>
        ) : (
          <div key={r.label} className="bar-row">
            {content}
          </div>
        );
      })}
    </section>
  );
}

function backupText(backup: BackupInfo | null) {
  if (!backup) return null;
  if (!backup.dir) return { title: t("백업 폴더가 없습니다"), body: t("PC가 고장 나면 북마크를 잃습니다. 백업 폴더를 정해 두세요.") };
  if (backup.lastBackup === null) return { title: t("아직 백업한 적이 없습니다"), body: t("백업 폴더는 정해져 있습니다. 지금 한 번 백업해 두세요.") };
  const days = Math.floor((Date.now() / 1000 - backup.lastBackup) / 86400);
  if (days < 3) return null;
  return { title: t("마지막 백업 {0}일 전", n(days)), body: t("앱이 켜져 있으면 하루에 한 번 자동으로 백업합니다. 오래됐다면 지금 백업하세요.") };
}

/** 얼마나 모았고, 얼마나 쓰고 있고, 무엇을 정리해야 하는지 보여주는 화면. */
export function Dashboard({
  counts,
  onFilter,
  onShow,
  onOpen,
  onImport,
  onBackup,
}: {
  /** 사이드바와 같은 집계. 사이트·태그 상위에 쓰고, 값이 바뀌면 대시보드도 다시 읽는다. */
  counts: SidebarCounts | null;
  /** 이 조건으로 북마크 목록을 연다. */
  onFilter: (patch: Partial<Filter>) => void;
  /** 북마크 목록에서 이 북마크의 편집 패널을 연다. */
  onShow: (bookmark: Bookmark) => void;
  onOpen: (bookmark: Bookmark) => void;
  onImport: () => void;
  onBackup: () => void;
}) {
  const [data, setData] = useState<DashboardData | null>(null);
  const [backup, setBackup] = useState<BackupInfo | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    invoke<DashboardData>("dashboard").then(setData).catch((e) => setError(errorText(e)));
    invoke<BackupInfo>("backup_status").then(setBackup).catch(() => {});
  }, [counts]);

  if (error) return <main className="content dash"><p className="hint error">{error}</p></main>;
  if (!data) return <main className="content dash" aria-busy="true" />;

  if (data.total === 0) {
    return (
      <main className="content dash">
        <h1>{t("대시보드")}</h1>
        <section className="dash-card">
          <p>{t("아직 북마크가 없습니다. 브라우저에서 가져오면 여기에 사용 현황이 나타납니다.")}</p>
          <button type="button" className="button primary start" onClick={onImport}>
            {t("북마크 가져오기")}
          </button>
        </section>
      </main>
    );
  }

  const diff = data.recent - data.previous;
  const stats = [
    { label: t("전체 북마크"), value: data.total, note: t("사이트 {0}곳 · 태그 {1}개", n(data.sites), n(data.tags)), go: () => onFilter({}) },
    {
      label: t("최근 7일 추가"),
      value: data.recent,
      note: diff === 0 ? t("지난주와 같음") : diff > 0 ? t("지난주보다 {0}개 많음", n(diff)) : t("지난주보다 {0}개 적음", n(-diff)),
      go: () => onFilter({ quick: "recent" }),
    },
    {
      label: t("한 번도 안 연 것"),
      value: data.unopened,
      note: t("전체의 {0}%", Math.round((data.unopened / data.total) * 100)),
      go: () => onFilter({ quick: "unopened" }),
    },
    { label: t("태그 없음"), value: data.untagged, note: t("분류가 필요한 북마크"), go: () => onFilter({ quick: "untagged" }) },
  ];

  const byMonth = new Map(data.months.map((m) => [m.month, m.count]));
  const months = lastMonths().map((m) => ({ ...m, count: byMonth.get(m.key) ?? 0 }));
  const cap = chartCap(months.map((m) => m.count));
  const topMax = Math.max(0, ...data.top.map((b) => b.openCount));

  const backupNote = backupText(backup);
  const todos = [
    data.stale > 0 && {
      title: t("6개월 넘게 안 연 북마크 {0}개", n(data.stale)),
      body: t("더 이상 필요 없는 것일 수 있습니다. 훑어보고 지우거나 남기세요."),
      action: t("목록 열기"),
      go: () => onFilter({ quick: "stale" }),
    },
    data.singletons > 0 && {
      title: t("사이트에 하나뿐인 북마크 {0}개", n(data.singletons)),
      body: t("그 사이트에서 저장한 것이 이것 하나뿐인 북마크입니다. 한 번 보고 만 곳인지 훑어보세요."),
      action: t("목록 열기"),
      go: () => onFilter({ quick: "singleton" }),
    },
    data.unrated > 0 && {
      title: t("별점 없는 북마크 {0}개", n(data.unrated)),
      body: t("자주 여는 것부터 별점을 매기면 나중에 찾기 쉽습니다."),
      action: t("평가하러 가기"),
      go: () => onFilter({ quick: "unrated", sort: "opens" }),
    },
    data.untagged > 0 && {
      title: t("태그 없는 북마크 {0}개", n(data.untagged)),
      body: t("사이트별 묶음은 자동이지만 주제별로 보려면 태그가 필요합니다."),
      action: t("태그 붙이기"),
      go: () => onFilter({ quick: "untagged" }),
    },
    data.noThumb > 0 && {
      title: t("썸네일 없는 북마크 {0}개", n(data.noThumb)),
      body: t("카드 보기에서 알아보기 쉽도록, 자주 여는 것부터 썸네일을 붙여 보세요."),
      action: t("썸네일 붙이기"),
      go: () => onFilter({ quick: "noThumb", sort: "opens" }),
    },
    backupNote && { ...backupNote, action: t("백업 설정 열기"), go: onBackup },
  ].filter((todo) => todo !== false && todo !== null) as { title: string; body: string; action: string; go: () => void }[];

  return (
    <main className="content dash">
      <div className="dash-head">
        <h1>{t("대시보드")}</h1>
        <span>{t("북마크를 얼마나 모으고, 얼마나 쓰고 있는지 한눈에 봅니다.")}</span>
      </div>

      <div className="stat-grid">
        {stats.map((s) => (
          <button key={s.label} type="button" className="stat" onClick={s.go}>
            <span className="section-label">{s.label}</span>
            <span className="stat-value">{n(s.value)}</span>
            <span className="hint">{s.note}</span>
          </button>
        ))}
      </div>

      <div className="dash-split">
        <section className="dash-card wide">
          <div className="dash-card-head">
            <h2>{t("자주 여는 북마크")}</h2>
            {data.top.length > 0 && (
              <button type="button" className="text-link" onClick={() => onFilter({ sort: "opens" })}>
                {t("전체 보기")}
              </button>
            )}
          </div>
          {data.top.length === 0 && (
            <p className="hint">{t("앱에서 북마크를 열면 여기에 자주 여는 순서로 나타납니다.")}</p>
          )}
          {data.top.map((b) => (
            <div key={b.id} className="top-row">
              <button type="button" className="top-main" onClick={() => onShow(b)}>
                <ThumbBox bookmark={b} className="top-thumb" />
                <span className="top-text">
                  <span className="title ellipsis">{b.title || b.url}</span>
                  <span className="mono ellipsis">{hostOf(b.url)}</span>
                </span>
                <span className="bar-track">
                  <span className="bar-fill" style={{ width: share(b.openCount, topMax) }} />
                </span>
                <span className="top-count mono">{t("{0}회", n(b.openCount))}</span>
              </button>
              <button type="button" className="icon-button" aria-label={t("브라우저에서 열기")} title={t("브라우저에서 열기")} onClick={() => onOpen(b)}>
                <Icon name="open" />
              </button>
            </div>
          ))}
        </section>

        <section className="dash-card">
          <h2>{t("월별 추가 수")}</h2>
          <div className="months" role="img" aria-label={t("최근 12개월 추가 수: {0}", months.map((m) => t("{0} {1}개", m.label, m.count)).join(", "))}>
            {months.map((m) => (
              <div key={m.key} className="month" title={t("{0}: {1}개", m.key, n(m.count))}>
                <span className="mono">{m.count > 0 ? n(m.count) : ""}</span>
                <span
                  className={m.count > cap ? "month-bar clipped" : "month-bar"}
                  style={{ height: m.count === 0 ? "2px" : share(Math.min(m.count, cap), cap) }}
                />
                <span className="mono">{m.label}</span>
              </div>
            ))}
          </div>
          {months.some((m) => m.count > cap) && (
            <p className="hint">{t("회색 막대는 한꺼번에 가져온 달이라 높이를 줄여 표시했습니다.")}</p>
          )}
        </section>
      </div>

      <div className="dash-grid">
        <Bars
          title={t("사이트 상위")}
          rows={(counts?.sites ?? []).slice(0, 5).map((s) => ({ label: s.name, count: s.count, onClick: () => onFilter({ host: s.name }) }))}
        />
        <Bars
          title={t("태그 상위")}
          rows={(counts?.tags ?? []).slice(0, 5).map((t) => ({ label: `#${t.name}`, count: t.count, onClick: () => onFilter({ tag: t.name }) }))}
        />
        <Bars
          title={t("별점 분포")}
          color="var(--star)"
          rows={[
            ...[5, 4, 3, 2, 1].map((stars) => ({ label: `★ ${stars}`, count: data.ratings[stars] })),
            { label: t("평가 없음"), count: data.ratings[0], onClick: () => onFilter({ quick: "unrated" }) },
          ]}
        />
      </div>

      <section className="dash-card">
        <h2>{t("정리할 것")}</h2>
        {todos.length === 0 ? (
          <p className="hint">{t("지금은 정리할 것이 없습니다.")}</p>
        ) : (
          <div className="todo-grid">
            {todos.map((todo) => (
              <div key={todo.title} className="todo">
                <div className="title">{todo.title}</div>
                <p className="hint">{todo.body}</p>
                <button type="button" className="button start" onClick={todo.go}>
                  {todo.action}
                </button>
              </div>
            ))}
          </div>
        )}
      </section>
    </main>
  );
}
