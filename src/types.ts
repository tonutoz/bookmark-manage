import { locale, t } from "./i18n";

export type Bookmark = {
  id: number;
  url: string;
  title: string;
  note: string;
  rating: number | null;
  tags: string[];
  createdAt: number;
  thumbVersion: number | null;
  /** 앱에서 이 북마크를 연 횟수 */
  openCount: number;
  /** 마지막으로 연 시각 (unix 초) */
  lastOpenedAt: number | null;
};

export type Workspace = { id: string; name: string };
/** 워크스페이스 목록과 지금 열려 있는 것의 id */
export type WorkspaceList = { active: string; items: Workspace[] };
/** 처음부터 있는 워크스페이스. 지울 수 없다. */
export const DEFAULT_WORKSPACE = "default";

/** 화면에 보여줄 워크스페이스 이름. 처음부터 있는 것은 이름을 바꾸지 않았다면 화면 언어로 보여준다. */
export const workspaceName = (w: Workspace) => (w.id === DEFAULT_WORKSPACE && w.name === "개인" ? t("개인") : w.name);

export type SaveResult = { bookmark: Bookmark; duplicate: boolean };

export type SidebarCounts = {
  total: number;
  starred: number;
  untagged: number;
  recent: number;
  tags: { name: string; count: number }[];
  /** 북마크가 많은 사이트(도메인) 순서 */
  sites: { name: string; count: number }[];
};

/** 사이드바의 빠른 필터와, 대시보드에서 넘어올 때 쓰는 조건들. */
export type Quick = "all" | "starred" | "untagged" | "recent" | "unopened" | "unrated" | "stale" | "singleton" | "noThumb";
export type Sort = "recent" | "oldest" | "rating" | "title" | "opens";
export type Layout = "grid" | "list";

export type Filter = { query: string; tag: string | null; host: string | null; quick: Quick; sort: Sort };

export const DEFAULT_FILTER: Filter = { query: "", tag: null, host: null, quick: "all", sort: "recent" };

/** "최근 추가"의 기준: 7일 전 (unix 초). */
export const recentSince = () => Math.floor(Date.now() / 1000) - 7 * 24 * 60 * 60;

/** "오래 안 연 것"의 기준: 180일 전 (unix 초). 백엔드의 STALE_AFTER 와 같다. */
export const staleBefore = () => Math.floor(Date.now() / 1000) - 180 * 24 * 60 * 60;

/** 화면의 필터를 백엔드 명령의 인자 모양으로 바꾼다. */
export const toBackendFilter = (f: Filter) => ({
  query: f.query,
  tag: f.tag,
  host: f.host,
  minRating: f.quick === "starred" ? 4 : null,
  untagged: f.quick === "untagged",
  singleton: f.quick === "singleton",
  unopened: f.quick === "unopened",
  unrated: f.quick === "unrated",
  noThumb: f.quick === "noThumb",
  staleBefore: f.quick === "stale" ? staleBefore() : null,
  addedSince: f.quick === "recent" ? recentSince() : null,
  sort: f.sort,
});

export const isFiltered = (f: Filter) => Boolean(f.query.trim() || f.tag || f.host || f.quick !== "all");

export function dateLabel(secs: number) {
  const date = new Date(secs * 1000);
  const sameYear = date.getFullYear() === new Date().getFullYear();
  return date.toLocaleDateString(
    locale,
    sameYear ? { month: "long", day: "numeric" } : { year: "numeric", month: "short", day: "numeric" },
  );
}

export const hostOf = (url: string) => {
  try {
    return new URL(url).host.replace(/^www\./, "") || url;
  } catch {
    return url;
  }
};

export const parseTags = (text: string) =>
  text
    .split(/[\s,]+/)
    .map((t) => t.replace(/^#+/, ""))
    .filter(Boolean);
