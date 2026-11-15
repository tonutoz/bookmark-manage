import { FormEvent, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Icon } from "./Icon";
import { Stars } from "./Stars";
import { ThumbBox, ThumbEditor } from "./Thumbnail";
import { Bookmark, Workspace, WorkspaceList, dateLabel, parseTags } from "./types";
import { MoveSelect } from "./WorkspaceMenu";
import { t, errorText } from "./i18n";

const same = (a: string[], b: string[]) => a.length === b.length && a.every((t, i) => t === b[i]);

/**
 * 선택한 북마크의 편집 패널. 부모가 key={bookmark.id} 로 넘겨, 다른 북마크를 고르면 입력값이 새로 시작된다.
 * 별점과 썸네일은 바로 저장되고, 제목·주소·태그·메모는 "저장"을 눌러야 반영된다.
 */
export function DetailPanel({
  bookmark,
  allTags,
  onChange,
  onOpen,
  onDelete,
  onMove,
  workspaces,
  onClose,
}: {
  bookmark: Bookmark;
  allTags: string[];
  /** 저장된 최신 값을 목록에 반영한다. tagsChanged 면 사이드바 개수도 다시 읽는다. */
  onChange: (updated: Bookmark, countsChanged: boolean) => void;
  onOpen: (bookmark: Bookmark) => void;
  onDelete: (id: number) => void;
  onMove: (id: number, target: Workspace) => void;
  workspaces: WorkspaceList | null;
  onClose: () => void;
}) {
  const [title, setTitle] = useState(bookmark.title);
  const [url, setUrl] = useState(bookmark.url);
  const [note, setNote] = useState(bookmark.note);
  const [tags, setTags] = useState(bookmark.tags);
  const [tagInput, setTagInput] = useState("");
  const [editingThumb, setEditingThumb] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const dirty =
    title !== bookmark.title || url !== bookmark.url || note !== bookmark.note || !same(tags, bookmark.tags) || tagInput.trim() !== "";

  function addTags(text: string) {
    const added = parseTags(text).filter((t) => !tags.some((have) => have.toLowerCase() === t.toLowerCase()));
    if (added.length) setTags([...tags, ...added]);
    setTagInput("");
    return [...tags, ...added];
  }

  async function save(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      // 입력창에 남아 있는 태그도 함께 저장한다.
      const finalTags = addTags(tagInput);
      const updated = await invoke<Bookmark>("update_bookmark", {
        id: bookmark.id,
        edit: { url, title, note, tags: finalTags },
      });
      setTitle(updated.title);
      setUrl(updated.url);
      setTags(updated.tags);
      onChange(updated, true);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  }

  async function rate(value: number | null) {
    try {
      onChange(await invoke<Bookmark>("set_rating", { id: bookmark.id, rating: value }), true);
    } catch (err) {
      setError(errorText(err));
    }
  }

  return (
    <aside className="detail" aria-label={t("북마크 편집")}>
      <div className="detail-head">
        <span className="section-label">{t("북마크 편집")}</span>
        <button type="button" className="icon-button" aria-label={t("편집 패널 닫기")} title={t("편집 패널 닫기")} onClick={onClose}>
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
            <path d="M6 6l12 12M18 6 6 18" />
          </svg>
        </button>
      </div>

      <div className="thumb-wrap">
        <ThumbBox bookmark={bookmark} className="detail-thumb" />
        <button
          type="button"
          className="icon-button thumb-edit"
          aria-label={t("썸네일 변경")}
          title={t("썸네일 변경")}
          aria-expanded={editingThumb}
          onClick={() => setEditingThumb(!editingThumb)}
        >
          <Icon name="image" />
        </button>
      </div>
      {editingThumb && <ThumbEditor bookmark={bookmark} onChange={(b) => onChange(b, false)} />}

      <form className="detail-form" onSubmit={save}>
        <label className="field">
          {t("제목")}
          <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} />
        </label>
        <label className="field">
          {t("주소")}
          <input className="input mono-input" value={url} onChange={(e) => setUrl(e.target.value)} required />
        </label>

        <div className="field">
          {t("별점")}
          <Stars value={bookmark.rating} onChange={rate} size={26} />
        </div>

        <div className="field">
          <label htmlFor="detail-tag-input">{t("태그")}</label>
          <div className="chips">
            {tags.map((tag) => (
              <span key={tag} className="chip">
                #{tag}
                <button type="button" aria-label={t("태그 {0} 제거", tag)} onClick={() => setTags(tags.filter((x) => x !== tag))}>
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" aria-hidden="true">
                    <path d="M6 6l12 12M18 6 6 18" />
                  </svg>
                </button>
              </span>
            ))}
          </div>
          <input
            id="detail-tag-input"
            className="input"
            list="all-tags"
            value={tagInput}
            placeholder={t("태그 입력 후 Enter")}
            onChange={(e) => setTagInput(e.target.value)}
            onKeyDown={(e) => {
              if ((e.key === "Enter" || e.key === ",") && tagInput.trim()) {
                e.preventDefault();
                addTags(tagInput);
              }
            }}
          />
          <datalist id="all-tags">
            {allTags.map((t) => (
              <option key={t} value={t} />
            ))}
          </datalist>
        </div>

        <label className="field">
          {t("메모")}
          <textarea className="input" rows={4} value={note} onChange={(e) => setNote(e.target.value)} />
        </label>

        {error && <p className="hint error">{error}</p>}
        {dirty && (
          <button type="submit" className="button primary" disabled={busy}>
            <Icon name="save" size={16} />
            {t("변경 내용 저장")}
          </button>
        )}
      </form>

      <p className="hint usage">
        {bookmark.openCount > 0
          ? t("{0}회 열어봄", bookmark.openCount.toLocaleString()) +
            (bookmark.lastOpenedAt ? t(" · 마지막 {0}", dateLabel(bookmark.lastOpenedAt)) : "")
          : t("아직 열어본 적 없음")}
        {t(" · 추가 {0}", dateLabel(bookmark.createdAt))}
      </p>

      <MoveSelect list={workspaces} onMove={(target) => onMove(bookmark.id, target)} />

      <div className="detail-actions">
        <button type="button" className={dirty ? "button grow" : "button primary grow"} onClick={() => onOpen(bookmark)}>
          <Icon name="open" size={16} />
          {t("브라우저에서 열기")}
        </button>
        <button type="button" className="icon-button danger bordered" aria-label={t("북마크 삭제")} title={t("북마크 삭제")} onClick={() => onDelete(bookmark.id)}>
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3" />
          </svg>
        </button>
      </div>
    </aside>
  );
}
