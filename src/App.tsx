import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AddDialog } from "./AddDialog";
import { BookmarkList } from "./BookmarkList";
import { ConfirmDelete, DeleteRequest } from "./ConfirmDelete";
import { Dashboard } from "./Dashboard";
import { Icon } from "./Icon";
import { DetailPanel } from "./DetailPanel";
import { EmptyState } from "./EmptyState";
import { Settings, SettingsTab } from "./Settings";
import { Sidebar } from "./Sidebar";
import { useTheme } from "./theme";
import { MoveSelect, WorkspaceMenu } from "./WorkspaceMenu";
import {
  Bookmark,
  DEFAULT_FILTER,
  Filter,
  Layout,
  SidebarCounts,
  Sort,
  Workspace,
  WorkspaceList,
  workspaceName,
  isFiltered,
  recentSince,
  toBackendFilter,
} from "./types";
import "./App.css";
import { t, errorText } from "./i18n";

/** 한 번에 읽는 개수. 스크롤이 끝에 가까워지면 다음 묶음을 이어 읽는다. */
const PAGE = 200;
/** 삭제 후 되돌릴 수 있는 시간 (ms). 이 시간이 지나야 실제로 지운다. 여러 개를 지울 때는 더 길게 준다. */
const UNDO_MS = 5000;
const UNDO_MANY_MS = 10000;
const NONE: Set<number> = new Set();
/** 이 개수 이상을 골라 지울 때는 문구를 입력해야 한다. */
const CONFIRM_FROM = 20;
/** 사이트 하나만 지우는 경우가 아닐 때 입력받는 문구 */
const GENERIC_PHRASE = t("삭제");
const LAYOUT_KEY = "bookmark-manage.layout";

type Page = { name: "main" } | { name: "dashboard" } | { name: "settings"; tab: SettingsTab };
type Toast = { text: string; undo?: () => void };

function storedLayout(): Layout {
  try {
    return localStorage.getItem(LAYOUT_KEY) === "list" ? "list" : "grid";
  } catch {
    return "grid";
  }
}

function describe(filter: Filter) {
  const parts = [];
  if (filter.host) parts.push(filter.host);
  if (filter.tag) parts.push(`#${filter.tag}`);
  if (filter.quick === "starred") parts.push(t("별 4개 이상"));
  if (filter.quick === "untagged") parts.push(t("태그 없음"));
  if (filter.quick === "recent") parts.push(t("최근 7일"));
  if (filter.quick === "unopened") parts.push(t("한 번도 안 연 것"));
  if (filter.quick === "unrated") parts.push(t("별점 없음"));
  if (filter.quick === "stale") parts.push(t("6개월 넘게 안 연 것"));
  if (filter.quick === "singleton") parts.push(t("사이트에 하나뿐인 것"));
  if (filter.quick === "noThumb") parts.push(t("썸네일 없음"));
  if (filter.query.trim()) parts.push(t("\"{0}\" 검색", filter.query.trim()));
  return parts.length ? parts.join(" · ") : t("전체");
}

function App() {
  const [page, setPage] = useState<Page>({ name: "main" });
  const [filter, setFilter] = useState<Filter>(DEFAULT_FILTER);
  const [layout, setLayout] = useState<Layout>(storedLayout);
  const [theme, setTheme] = useTheme();
  const [items, setItems] = useState<Bookmark[]>([]);
  const [total, setTotal] = useState(0);
  const [counts, setCounts] = useState<SidebarCounts | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceList | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [adding, setAdding] = useState(false);
  const [toast, setToast] = useState<Toast | null>(null);
  // 삭제를 누른 뒤 되돌리기 시간이 지나기 전까지 화면에서만 숨겨 두는 북마크들
  const [hidden, setHidden] = useState<Set<number>>(NONE);
  // 여러 개를 골라 지우는 선택 모드와 고른 북마크들
  const [picking, setPicking] = useState(false);
  const [picked, setPicked] = useState<Set<number>>(NONE);
  // 문구 입력을 기다리는 대량 삭제 요청
  const [confirming, setConfirming] = useState<DeleteRequest | null>(null);

  // 검색어가 바뀌면 번호를 올려, 늦게 도착한 이전 검색의 응답을 버린다.
  const generation = useRef(0);
  const loadingMore = useRef(false);
  const pendingDelete = useRef<{ ids: number[]; timer: number } | null>(null);
  const searchInput = useRef<HTMLInputElement>(null);

  const showError = useCallback((e: unknown) => setToast({ text: errorText(e) }), []);

  const loadCounts = useCallback(
    () => invoke<SidebarCounts>("sidebar_counts", { recentSince: recentSince() }).then(setCounts).catch(showError),
    [showError],
  );

  const reload = useCallback(async () => {
    const mine = ++generation.current;
    const backend = toBackendFilter(filter);
    try {
      const [count, first] = await Promise.all([
        invoke<number>("count_bookmarks", { filter: backend }),
        invoke<Bookmark[]>("list_bookmarks", { filter: { ...backend, limit: PAGE, offset: 0 } }),
      ]);
      if (mine !== generation.current) return;
      setTotal(count);
      setItems(first);
    } catch (e) {
      showError(e);
    }
  }, [filter, showError]);

  const loadMore = useCallback(async () => {
    if (loadingMore.current || items.length >= total) return;
    loadingMore.current = true;
    const mine = generation.current;
    try {
      const next = await invoke<Bookmark[]>("list_bookmarks", {
        filter: { ...toBackendFilter(filter), limit: PAGE, offset: items.length },
      });
      if (mine !== generation.current) return;
      // 읽는 사이 새 북마크가 들어와 순서가 밀렸을 때 같은 항목이 두 번 들어가지 않게 한다.
      setItems((list) => {
        const known = new Set(list.map((b) => b.id));
        return [...list, ...next.filter((b) => !known.has(b.id))];
      });
      if (next.length === 0) setTotal(items.length);
    } catch (e) {
      showError(e);
    } finally {
      loadingMore.current = false;
    }
  }, [items.length, total, filter, showError]);

  const loadWorkspaces = useCallback(
    () => invoke<WorkspaceList>("list_workspaces").then(setWorkspaces).catch(showError),
    [showError],
  );

  const refresh = useCallback(() => {
    reload();
    loadCounts();
    loadWorkspaces();
  }, [reload, loadCounts, loadWorkspaces]);

  // 타이머처럼 나중에 실행되는 코드가 항상 지금 필터 기준으로 다시 읽도록 최신 함수를 가리킨다.
  const latestRefresh = useRef(refresh);
  latestRefresh.current = refresh;

  // 입력할 때마다 조회하지 않도록 잠깐 기다렸다가 읽는다.
  useEffect(() => {
    const timer = setTimeout(reload, 150);
    return () => clearTimeout(timer);
  }, [reload]);

  useEffect(() => {
    loadCounts();
    loadWorkspaces();
  }, [loadCounts, loadWorkspaces]);

  // 트레이 메뉴는 백엔드가 만들지만 언어는 화면이 알고 있어서, 화면이 뜨면 글자를 맞춰 준다.
  useEffect(() => {
    invoke("set_tray_labels", { open: t("열기"), autostart: t("윈도우 시작 시 실행"), quit: t("종료") }).catch(() => {});
  }, []);

  // 브라우저 확장이 저장했거나 백업을 복원하면 다시 읽는다.
  useEffect(() => {
    const unlisten = listen("bookmarks-changed", refresh);
    return () => {
      unlisten.then((stop) => stop());
    };
  }, [refresh]);

  useEffect(() => {
    if (!toast || toast.undo) return;
    const timer = setTimeout(() => setToast(null), 4000);
    return () => clearTimeout(timer);
  }, [toast]);

  /** 되돌리기 대기 중인 삭제가 있으면 지금 실제로 지운다. */
  const flushDelete = useCallback(async () => {
    const pending = pendingDelete.current;
    if (!pending) return;
    pendingDelete.current = null;
    clearTimeout(pending.timer);
    const gone = new Set(pending.ids);
    try {
      await invoke("delete_bookmarks", { ids: pending.ids });
      setItems((list) => list.filter((b) => !gone.has(b.id)));
    } catch (e) {
      showError(e);
    }
    setHidden(NONE);
    // 이미 지웠으므로 되돌리기 알림은 거둔다.
    setToast((current) => (current?.undo ? null : current));
    latestRefresh.current();
  }, [showError]);

  /** 화면에서 먼저 숨기고, 되돌리기 시간이 지나면 실제로 지운다. */
  async function remove(ids: number[]) {
    if (ids.length === 0) return;
    await flushDelete();
    setHidden(new Set(ids));
    setSelectedId(null);
    setPicking(false);
    setPicked(NONE);
    const timer = window.setTimeout(
      () => {
        setToast(null);
        flushDelete();
      },
      ids.length > 1 ? UNDO_MANY_MS : UNDO_MS,
    );
    pendingDelete.current = { ids, timer };
    setToast({
      text: ids.length > 1 ? t("북마크 {0}개를 삭제했습니다", ids.length.toLocaleString()) : t("북마크를 삭제했습니다"),
      undo: () => {
        clearTimeout(timer);
        pendingDelete.current = null;
        setHidden(NONE);
        setToast(null);
      },
    });
  }

  /** 지금 조건(사이트, 태그, 검색 등)에 맞는 북마크의 id 전부. 아직 화면에 읽어오지 않은 것도 포함한다. */
  const matchingIds = () => invoke<number[]>("list_bookmark_ids", { filter: toBackendFilter(filter) });

  /** 확인 창에 보여줄 요청을 만든다. 지워질 것 중 화면에 읽어온 앞쪽 몇 개를 미리 보기로 쓴다. */
  function askToRemove(ids: number[], target: string, phrase: string) {
    const wanted = new Set(ids);
    const samples = items
      .filter((b) => wanted.has(b.id))
      .slice(0, 5)
      .map((b) => b.title || b.url);
    setConfirming({ ids, target, phrase, samples });
  }

  /** 지금 조건에 맞는 것을 모두 지운다. 사이트 하나만 고른 상태면 그 도메인을 입력해야 한다. */
  async function removeMatching() {
    try {
      const onlySite = filter.host && !filter.tag && !filter.query.trim() && filter.quick === "all";
      askToRemove(await matchingIds(), describe(filter), onlySite ? filter.host! : GENERIC_PHRASE);
    } catch (e) {
      showError(e);
    }
  }

  /** 고른 것을 지운다. 많이 골랐으면 한 번 더 확인한다. */
  function removePicked() {
    const ids = [...picked];
    if (ids.length >= CONFIRM_FROM) askToRemove(ids, t("선택한 북마크"), GENERIC_PHRASE);
    else void remove(ids);
  }

  function togglePick(id: number) {
    setPicked((current) => {
      const next = new Set(current);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  }

  // 조건을 바꾸면 대기 중인 삭제를 바로 마무리해, 새 조건의 개수가 어긋나지 않게 한다.
  useEffect(() => {
    void flushDelete();
  }, [filter, flushDelete]);

  // 창을 닫을 때 대기 중인 삭제를 마무리한다.
  useEffect(() => {
    const onHide = () => void flushDelete();
    window.addEventListener("beforeunload", onHide);
    return () => window.removeEventListener("beforeunload", onHide);
  }, [flushDelete]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPage({ name: "main" });
        // 설정 화면에 있었다면 검색창이 다시 그려진 뒤에 포커스를 준다.
        setTimeout(() => searchInput.current?.focus(), 0);
      } else if (mod && e.key.toLowerCase() === "n") {
        e.preventDefault();
        setAdding(true);
      } else if (e.key === "Escape") {
        if (confirming) setConfirming(null);
        else if (adding) setAdding(false);
        else if (picking) {
          setPicking(false);
          setPicked(NONE);
        } else setSelectedId(null);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [adding, picking, confirming]);

  function changeLayout(next: Layout) {
    setLayout(next);
    try {
      localStorage.setItem(LAYOUT_KEY, next);
    } catch {
      // 저장하지 못해도 이번 실행에서는 그대로 쓴다.
    }
  }

  const patchFilter = (patch: Partial<Filter>) => {
    setFilter((f) => ({ ...f, ...patch }));
    setSelectedId(null);
    // 조건이 바뀌면 고른 것이 화면에서 사라질 수 있으므로 선택을 비운다.
    setPicked(NONE);
  };

  /** 브라우저로 열고, 열어본 횟수를 하나 올린다. */
  const open = async (bookmark: Bookmark) => {
    try {
      const updated = await invoke<Bookmark>("open_bookmark", { id: bookmark.id });
      setItems((list) => list.map((b) => (b.id === updated.id ? updated : b)));
      await openUrl(bookmark.url);
    } catch (e) {
      showError(e);
    }
  };

  /** 다른 워크스페이스로 전환한다. 화면의 조건과 선택은 처음 상태로 돌린다. */
  async function switchWorkspace(id: string) {
    // 되돌리기 대기 중인 삭제는 지금 워크스페이스의 것이므로 넘어가기 전에 마무리한다.
    await flushDelete();
    try {
      await invoke("switch_workspace", { id });
    } catch (e) {
      showError(e);
      return;
    }
    setItems([]);
    setTotal(0);
    setCounts(null);
    setSelectedId(null);
    setPicking(false);
    setPicked(NONE);
    // 조건이 이미 처음 상태면 값이 안 바뀌어 다시 읽지 않으므로 직접 읽는다.
    setFilter(DEFAULT_FILTER);
    latestRefresh.current();
  }

  /** 북마크들을 다른 워크스페이스로 보낸다. */
  async function moveTo(ids: number[], target: Workspace) {
    if (ids.length === 0) return;
    await flushDelete();
    try {
      const moved = await invoke<number>("move_bookmarks", { ids, target: target.id });
      setSelectedId(null);
      setPicking(false);
      setPicked(NONE);
      setToast({ text: t("북마크 {0}개를 {1} 워크스페이스로 보냈습니다", moved.toLocaleString(), workspaceName(target)) });
      refresh();
    } catch (e) {
      showError(e);
    }
  }

  /** 북마크 화면으로 가서 이 북마크의 편집 패널을 연다. 지금 조건의 목록에 없을 수 있으니 필터를 푼다. */
  function showBookmark(bookmark: Bookmark) {
    setPage({ name: "main" });
    setFilter(DEFAULT_FILTER);
    setPicking(false);
    setItems((list) => [bookmark, ...list.filter((b) => b.id !== bookmark.id)]);
    setSelectedId(bookmark.id);
  }

  const visible = hidden.size === 0 ? items : items.filter((b) => !hidden.has(b.id));
  const shownTotal = Math.max(0, total - hidden.size);
  const selected = picking ? null : (visible.find((b) => b.id === selectedId) ?? null);
  const filtered = isFiltered(filter);
  const firstRun = counts !== null && counts.total === 0 && !filtered;

  return (
    <div className="shell">
      <Sidebar
        header={<WorkspaceMenu list={workspaces} onSwitch={switchWorkspace} onChanged={loadWorkspaces} />}
        counts={counts}
        filter={filter}
        onMain={page.name === "main"}
        dashboardActive={page.name === "dashboard"}
        settingsActive={page.name === "settings"}
        onDashboard={() => setPage({ name: "dashboard" })}
        onPick={(patch) => {
          patchFilter(patch);
          setPage({ name: "main" });
        }}
        onSettings={() => setPage({ name: "settings", tab: "import" })}
      />

      {page.name === "dashboard" ? (
        <Dashboard
          counts={counts}
          onFilter={(patch) => {
            // 대시보드의 숫자는 다른 조건 없이 센 것이므로, 필터를 처음 상태로 돌린 뒤 그 조건만 건다.
            setFilter({ ...DEFAULT_FILTER, ...patch });
            setSelectedId(null);
            setPicked(NONE);
            setPage({ name: "main" });
          }}
          onShow={showBookmark}
          onOpen={open}
          onImport={() => setPage({ name: "settings", tab: "import" })}
          onBackup={() => setPage({ name: "settings", tab: "backup" })}
        />
      ) : page.name === "settings" ? (
        <Settings
          tab={page.tab}
          onTab={(tab) => setPage({ name: "settings", tab })}
          exportFilter={toBackendFilter(filter)}
          exportFiltered={filtered}
          onImported={refresh}
          theme={theme}
          onTheme={setTheme}
        />
      ) : (
        <div className="workspace">
          <main className="content">
            <div className="toolbar">
              <div className="search">
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
                  <circle cx="11" cy="11" r="7" />
                  <path d="m20 20-3.5-3.5" />
                </svg>
                <input
                  ref={searchInput}
                  aria-label={t("검색")}
                  value={filter.query}
                  onChange={(e) => patchFilter({ query: e.target.value })}
                  placeholder={t("제목, 주소, 메모, 태그 검색")}
                />
                <kbd>Ctrl K</kbd>
              </div>
              <select
                className="input"
                aria-label={t("정렬")}
                value={filter.sort}
                onChange={(e) => patchFilter({ sort: e.target.value as Sort })}
              >
                <option value="recent">{t("최근 추가순")}</option>
                <option value="oldest">{t("오래된 순")}</option>
                <option value="rating">{t("별점 높은순")}</option>
                <option value="title">{t("제목순")}</option>
                <option value="opens">{t("자주 여는 순")}</option>
              </select>
              <div className="segmented">
                <button type="button" aria-label={t("카드 보기")} aria-pressed={layout === "grid"} onClick={() => changeLayout("grid")}>
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
                    <rect x="4" y="4" width="7" height="7" rx="1" />
                    <rect x="13" y="4" width="7" height="7" rx="1" />
                    <rect x="4" y="13" width="7" height="7" rx="1" />
                    <rect x="13" y="13" width="7" height="7" rx="1" />
                  </svg>
                </button>
                <button type="button" aria-label={t("목록 보기")} aria-pressed={layout === "list"} onClick={() => changeLayout("list")}>
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
                    <path d="M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01" />
                  </svg>
                </button>
              </div>
              <button
                type="button"
                className="button primary icon-only"
                aria-label={t("북마크 추가")}
                title={t("북마크 추가 (Ctrl+N)")}
                onClick={() => setAdding(true)}
              >
                <Icon name="plus" />
              </button>
            </div>

            {firstRun ? (
              <EmptyState
                onImported={refresh}
                onAdd={() => setAdding(true)}
                onConnectExtension={() => setPage({ name: "settings", tab: "extension" })}
              />
            ) : (
              <>
                {picking ? (
                  <div className="summary picking">
                    <span>{t("{0}개 선택", picked.size.toLocaleString())}</span>
                    <button
                      type="button"
                      className="pill"
                      onClick={() => matchingIds().then((ids) => setPicked(new Set(ids))).catch(showError)}
                    >
                      {t("전체 선택 ({0}개)", shownTotal.toLocaleString())}
                    </button>
                    <button type="button" className="pill" disabled={picked.size === 0} onClick={() => setPicked(NONE)}>
                      {t("선택 해제")}
                    </button>
                    <span className="grow" />
                    <MoveSelect list={workspaces} disabled={picked.size === 0} onMove={(target) => moveTo([...picked], target)} />
                    <button type="button" className="pill danger" disabled={picked.size === 0} onClick={removePicked}>
                      <Icon name="trash" size={14} />
                      {t("선택 삭제")}
                    </button>
                    <button
                      type="button"
                      className="icon-button small"
                      aria-label={t("선택 모드 끝내기")}
                      title={t("선택 모드 끝내기 (Esc)")}
                      onClick={() => {
                        setPicking(false);
                        setPicked(NONE);
                      }}
                    >
                      <Icon name="close" size={16} />
                    </button>
                  </div>
                ) : (
                  <div className="summary">
                    <span>
                      {t("{0} · {1}개", describe(filter), shownTotal.toLocaleString())}
                    </span>
                    {filtered && (
                      <button type="button" className="pill" onClick={() => patchFilter({ query: "", tag: null, host: null, quick: "all" })}>
                        <Icon name="close" size={12} />
                        {t("필터 지우기")}
                      </button>
                    )}
                    <span className="grow" />
                    {filtered && shownTotal > 0 && (
                      <button
                        type="button"
                        className="icon-button small danger"
                        aria-label={t("이 조건에 맞는 북마크 모두 삭제")}
                        title={t("이 조건에 맞는 북마크 모두 삭제")}
                        onClick={removeMatching}
                      >
                        <Icon name="trash" size={16} />
                      </button>
                    )}
                    {shownTotal > 0 && (
                      <button
                        type="button"
                        className="icon-button small"
                        aria-label={t("여러 개 선택")}
                        title={t("여러 개 선택")}
                        onClick={() => {
                          setPicking(true);
                          setSelectedId(null);
                        }}
                      >
                        <Icon name="select" size={16} />
                      </button>
                    )}
                  </div>
                )}
                {shownTotal === 0 && filtered && <p className="no-results">{t("조건에 맞는 북마크가 없습니다.")}</p>}
                <BookmarkList
                  items={visible}
                  layout={layout}
                  selectedIds={picking ? picked : selectedId === null ? NONE : new Set([selectedId])}
                  checkable={picking}
                  resetKey={JSON.stringify(filter)}
                  onSelect={(id) => (picking ? togglePick(id) : setSelectedId(id === selectedId ? null : id))}
                  onOpen={picking ? () => {} : open}
                  onNeedMore={loadMore}
                />
              </>
            )}
          </main>

          {selected && (
            <DetailPanel
              key={selected.id}
              bookmark={selected}
              allTags={counts?.tags.map((t) => t.name) ?? []}
              onChange={(updated, countsChanged) => {
                setItems((list) => list.map((b) => (b.id === updated.id ? updated : b)));
                if (countsChanged) loadCounts();
              }}
              onOpen={open}
              onDelete={(id) => remove([id])}
              onMove={(id, target) => moveTo([id], target)}
              workspaces={workspaces}
              onClose={() => setSelectedId(null)}
            />
          )}
        </div>
      )}

      {adding && (
        <AddDialog
          onClose={() => setAdding(false)}
          onSaved={(bookmark) => {
            setAdding(false);
            setPage({ name: "main" });
            refresh();
            setItems((list) => [bookmark, ...list.filter((b) => b.id !== bookmark.id)]);
            setSelectedId(bookmark.id);
          }}
          onEditExisting={(bookmark) => {
            setAdding(false);
            setPage({ name: "main" });
            // 기존 항목이 지금 조건의 목록에 없을 수 있으니 필터를 풀고 맨 앞에 보여준 뒤 편집 패널을 연다.
            setFilter(DEFAULT_FILTER);
            setPicking(false);
            setItems((list) => [bookmark, ...list.filter((b) => b.id !== bookmark.id)]);
            setSelectedId(bookmark.id);
          }}
        />
      )}

      {confirming && (
        <ConfirmDelete
          request={confirming}
          onClose={() => setConfirming(null)}
          onConfirm={(ids) => {
            setConfirming(null);
            void remove(ids);
          }}
        />
      )}

      {toast && (
        <div className="toast" role="status">
          <span>{toast.text}</span>
          {toast.undo && (
            <button type="button" onClick={toast.undo}>
              {t("되돌리기")}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

export default App;
