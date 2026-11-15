import { ReactNode, useState } from "react";
import { Icon } from "./Icon";
import { Filter, Quick, SidebarCounts } from "./types";
import { t } from "./i18n";

const QUICK: { key: Quick; label: string; count: (c: SidebarCounts) => number }[] = [
  { key: "all", label: t("전체"), count: (c) => c.total },
  { key: "starred", label: t("별 4개 이상"), count: (c) => c.starred },
  { key: "untagged", label: t("태그 없음"), count: (c) => c.untagged },
  { key: "recent", label: t("최근 7일"), count: (c) => c.recent },
];

/** 사이트는 수백 개가 될 수 있어 처음에는 북마크가 많은 것만 보여준다. */
const SITES_SHOWN = 8;

function Item({
  label,
  count,
  active,
  onClick,
}: {
  label: string;
  count: number | null;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={active ? "nav-item active" : "nav-item"}
      aria-current={active ? "page" : undefined}
      onClick={onClick}
    >
      <span className="ellipsis">{label}</span>
      <span className="nav-count">{count === null ? "" : count.toLocaleString()}</span>
    </button>
  );
}

export function Sidebar({
  header,
  counts,
  filter,
  onMain,
  dashboardActive,
  settingsActive,
  onPick,
  onDashboard,
  onSettings,
}: {
  /** 맨 위에 놓는 워크스페이스 메뉴 */
  header: ReactNode;
  counts: SidebarCounts | null;
  filter: Filter;
  /** 설정 화면이 아니라 북마크 화면을 보고 있는지 */
  onMain: boolean;
  dashboardActive: boolean;
  settingsActive: boolean;
  onDashboard: () => void;
  onPick: (patch: Partial<Filter>) => void;
  onSettings: () => void;
}) {
  const [allSites, setAllSites] = useState(false);
  const sites = counts?.sites ?? [];
  // 접혀 있어도 지금 고른 사이트는 항상 보이게 한다.
  const shownSites = allSites
    ? sites
    : sites.filter((s, index) => index < SITES_SHOWN || s.name === filter.host);

  return (
    <nav className="sidebar" aria-label={t("필터, 사이트, 태그")}>
      {header}

      <button
        type="button"
        className={dashboardActive ? "nav-item active" : "nav-item"}
        aria-current={dashboardActive ? "page" : undefined}
        onClick={onDashboard}
      >
        <span className="with-icon">
          <Icon name="dashboard" />
          {t("대시보드")}
        </span>
      </button>

      {QUICK.map((q) => (
        <Item
          key={q.key}
          label={q.label}
          count={counts ? q.count(counts) : null}
          active={onMain && !filter.tag && !filter.host && filter.quick === q.key}
          onClick={() => onPick({ quick: q.key, tag: null, host: null })}
        />
      ))}

      <div className="nav-scroll">
        {sites.length > 0 && (
          <>
            <div className="nav-label">{t("사이트")}</div>
            {shownSites.map((s) => {
              const active = onMain && filter.host === s.name;
              return (
                <Item
                  key={s.name}
                  label={s.name}
                  count={s.count}
                  active={active}
                  onClick={() => onPick({ host: active ? null : s.name, quick: "all" })}
                />
              );
            })}
            {sites.length > SITES_SHOWN && (
              <button type="button" className="nav-more" aria-expanded={allSites} onClick={() => setAllSites(!allSites)}>
                {allSites ? t("접기") : t("사이트 {0}개 더 보기", (sites.length - SITES_SHOWN).toLocaleString())}
              </button>
            )}
          </>
        )}

        <div className="nav-label">{t("태그")}</div>
        {counts?.tags.length === 0 && <p className="nav-empty">{t("북마크에 태그를 붙이면 여기에 모입니다.")}</p>}
        {counts?.tags.map((t) => {
          const active = onMain && filter.tag === t.name;
          return (
            <Item
              key={t.name}
              label={`#${t.name}`}
              count={t.count}
              active={active}
              onClick={() => onPick({ tag: active ? null : t.name, quick: "all" })}
            />
          );
        })}
      </div>

      <button
        type="button"
        className={settingsActive ? "nav-item active" : "nav-item"}
        aria-current={settingsActive ? "page" : undefined}
        onClick={onSettings}
      >
        <span className="with-icon">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
            <circle cx="12" cy="12" r="3" />
            <path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1 7 17M17 7l2.1-2.1" />
          </svg>
          {t("설정")}
        </span>
      </button>
    </nav>
  );
}
