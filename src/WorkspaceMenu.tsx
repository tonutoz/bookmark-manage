import { FormEvent, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Icon } from "./Icon";
import { DEFAULT_WORKSPACE, Workspace, WorkspaceList, workspaceName } from "./types";
import { t, tSplit, errorText } from "./i18n";

/**
 * 사이드바 맨 위의 워크스페이스 메뉴. 전환, 추가, 이름 변경, 삭제를 여기서 한다.
 * 목록을 고친 뒤에는 onChanged 로 부모가 목록을 다시 읽게 한다.
 */
export function WorkspaceMenu({
  list,
  onSwitch,
  onChanged,
}: {
  list: WorkspaceList | null;
  onSwitch: (id: string) => void;
  onChanged: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [newName, setNewName] = useState("");
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  const [deleting, setDeleting] = useState<Workspace | null>(null);
  const [typed, setTyped] = useState("");
  const [error, setError] = useState("");

  const active = list?.items.find((w) => w.id === list.active);
  // 굵게 보여줄 워크스페이스 이름의 앞뒤 문장
  const about = tSplit("{0} 워크스페이스와 그 안의 북마크, 태그, 썸네일을 모두 지웁니다. 되돌릴 수 없고, 백업에서 복원하는 방법만 남습니다.");
  const typeIt = tSplit("계속하려면 {0} 를 입력하세요");

  function close() {
    setOpen(false);
    setRenaming(null);
    setNewName("");
    setError("");
  }

  async function run(job: () => Promise<unknown>) {
    setError("");
    try {
      await job();
      onChanged();
      return true;
    } catch (e) {
      setError(errorText(e));
      return false;
    }
  }

  async function add(e: FormEvent) {
    e.preventDefault();
    let created: Workspace | undefined;
    const ok = await run(async () => {
      created = await invoke<Workspace>("create_workspace", { name: newName });
    });
    if (ok && created) {
      close();
      // 새로 만든 곳으로 바로 들어간다.
      onSwitch(created.id);
    }
  }

  async function rename(e: FormEvent) {
    e.preventDefault();
    if (!renaming) return;
    if (await run(() => invoke("rename_workspace", renaming))) setRenaming(null);
  }

  async function remove(e: FormEvent) {
    e.preventDefault();
    if (!deleting || typed.trim() !== deleting.name) return;
    if (await run(() => invoke("delete_workspace", { id: deleting.id }))) {
      setDeleting(null);
      setTyped("");
    }
  }

  return (
    <div className="workspace-switch">
      <button
        type="button"
        className="workspace-button"
        aria-haspopup="menu"
        aria-expanded={open}
        title={t("워크스페이스 전환")}
        onClick={() => (open ? close() : setOpen(true))}
      >
        <span className="workspace-text">
          <span className="section-label">{t("워크스페이스")}</span>
          <span className="workspace-name ellipsis">{active ? workspaceName(active) : "…"}</span>
        </span>
        <Icon name="chevron" size={16} />
      </button>

      {open && list && (
        <>
          <div className="menu-backdrop" onMouseDown={close} />
          <div className="workspace-menu" role="menu">
            {list.items.map((w) =>
              renaming?.id === w.id ? (
                <form key={w.id} className="workspace-row" onSubmit={rename}>
                  <input
                    className="input grow"
                    aria-label={t("워크스페이스 이름")}
                    value={renaming.name}
                    onChange={(e) => setRenaming({ id: w.id, name: e.target.value })}
                    autoFocus
                  />
                  <button type="submit" className="icon-button small" aria-label={t("이름 저장")} title={t("이름 저장")}>
                    <Icon name="save" size={16} />
                  </button>
                </form>
              ) : (
                <div key={w.id} className="workspace-row">
                  <button
                    type="button"
                    role="menuitemradio"
                    aria-checked={w.id === list.active}
                    className={w.id === list.active ? "nav-item active grow" : "nav-item grow"}
                    onClick={() => {
                      close();
                      if (w.id !== list.active) onSwitch(w.id);
                    }}
                  >
                    <span className="ellipsis">{workspaceName(w)}</span>
                  </button>
                  <button
                    type="button"
                    className="icon-button small"
                    aria-label={t("{0} 이름 바꾸기", w.name)}
                    title={t("이름 바꾸기")}
                    onClick={() => setRenaming({ id: w.id, name: w.name })}
                  >
                    <Icon name="edit" size={16} />
                  </button>
                  {w.id !== DEFAULT_WORKSPACE && w.id !== list.active && (
                    <button
                      type="button"
                      className="icon-button small danger"
                      aria-label={t("{0} 삭제", w.name)}
                      title={t("워크스페이스 삭제")}
                      onClick={() => {
                        setTyped("");
                        setDeleting(w);
                      }}
                    >
                      <Icon name="trash" size={16} />
                    </button>
                  )}
                </div>
              ),
            )}

            <form className="workspace-row add" onSubmit={add}>
              <input
                className="input grow"
                aria-label={t("새 워크스페이스 이름")}
                placeholder={t("새 워크스페이스 (예: 업무)")}
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                required
              />
              <button type="submit" className="icon-button small" aria-label={t("워크스페이스 추가")} title={t("워크스페이스 추가")}>
                <Icon name="plus" size={16} />
              </button>
            </form>
            {error && <p className="hint error">{error}</p>}
          </div>
        </>
      )}

      {deleting && (
        <div
          className="overlay"
          onMouseDown={(e) => {
            if (e.target === e.currentTarget) setDeleting(null);
          }}
        >
          <form className="dialog" role="alertdialog" aria-modal="true" aria-labelledby="ws-delete-title" onSubmit={remove}>
            <h2 id="ws-delete-title">{t("워크스페이스 삭제")}</h2>
            <p>
              {about[0]}
              <strong>{deleting.name}</strong>
              {about[1]}
            </p>
            <label className="field">
              <span>
                {typeIt[0]}
                <strong className="phrase">{deleting.name}</strong>
                {typeIt[1]}
              </span>
              <input className="input" value={typed} onChange={(e) => setTyped(e.target.value)} autoFocus autoComplete="off" />
            </label>
            {error && <p className="hint error">{error}</p>}
            <div className="button-row end">
              <button type="button" className="button" onClick={() => setDeleting(null)}>
                {t("취소")}
              </button>
              <button type="submit" className="button danger-fill" disabled={typed.trim() !== deleting.name}>
                {t("워크스페이스 삭제")}
              </button>
            </div>
          </form>
        </div>
      )}
    </div>
  );
}

/** 북마크를 다른 워크스페이스로 보내는 선택란. 보낼 곳이 없으면 아무것도 그리지 않는다. */
export function MoveSelect({
  list,
  disabled,
  onMove,
}: {
  list: WorkspaceList | null;
  disabled?: boolean;
  onMove: (target: Workspace) => void;
}) {
  const others = list?.items.filter((w) => w.id !== list.active) ?? [];
  if (others.length === 0) return null;
  return (
    <select
      className="input move-select"
      aria-label={t("다른 워크스페이스로 보내기")}
      value=""
      disabled={disabled}
      onChange={(e) => {
        const target = others.find((w) => w.id === e.target.value);
        if (target) onMove(target);
      }}
    >
      <option value="" disabled>
        {t("다른 워크스페이스로 보내기")}
      </option>
      {others.map((w) => (
        <option key={w.id} value={w.id}>
          {workspaceName(w)}
        </option>
      ))}
    </select>
  );
}
