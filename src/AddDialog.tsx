import { FormEvent, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Icon } from "./Icon";
import { Stars } from "./Stars";
import { ThumbBox } from "./Thumbnail";
import { Bookmark, SaveResult, dateLabel, hostOf, parseTags } from "./types";
import { t, errorText } from "./i18n";

/**
 * 북마크 직접 추가 창.
 * 주소를 입력하는 동안 이미 저장된 주소인지 확인하고, 중복이면 저장 대신 기존 북마크를 고치도록 안내한다.
 */
export function AddDialog({
  onSaved,
  onEditExisting,
  onClose,
}: {
  onSaved: (bookmark: Bookmark) => void;
  /** 중복이라 새로 만들지 않고 기존 북마크의 편집 패널을 연다. */
  onEditExisting: (bookmark: Bookmark) => void;
  onClose: () => void;
}) {
  const [url, setUrl] = useState("");
  const [title, setTitle] = useState("");
  const [tags, setTags] = useState("");
  const [note, setNote] = useState("");
  const [rating, setRating] = useState<number | null>(null);
  const [existing, setExisting] = useState<Bookmark | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  // 입력이 잠깐 멈추면 같은 주소가 이미 있는지 찾아본다.
  useEffect(() => {
    setExisting(null);
    const wanted = url.trim();
    if (!wanted) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      invoke<Bookmark | null>("find_bookmark", { url: wanted })
        .then((found) => {
          if (!cancelled) setExisting(found);
        })
        .catch(() => {});
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [url]);

  async function save(e: FormEvent) {
    e.preventDefault();
    if (existing) return;
    setBusy(true);
    setError("");
    try {
      const result = await invoke<SaveResult>("add_bookmark", {
        bookmark: { url, title, note, rating, tags: parseTags(tags) },
      });
      // 확인이 끝나기 전에 저장을 눌렀더라도 중복이면 새로 만들어지지 않는다.
      if (result.duplicate) {
        setExisting(result.bookmark);
        setBusy(false);
      } else onSaved(result.bookmark);
    } catch (err) {
      setError(errorText(err));
      setBusy(false);
    }
  }

  return (
    <div
      className="overlay"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <form className="dialog" role="dialog" aria-modal="true" aria-labelledby="add-title" onSubmit={save}>
        <h2 id="add-title">{t("북마크 추가")}</h2>
        <label className="field">
          {t("주소")}
          <input
            className="input mono-input"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://example.com/article"
            required
            autoFocus
          />
        </label>

        {existing ? (
          <div className="duplicate" role="alert">
            <div className="duplicate-title">{t("이미 저장된 주소입니다")}</div>
            <div className="duplicate-item">
              <ThumbBox bookmark={existing} className="row-thumb" />
              <div className="row-main">
                <span className="title ellipsis">{existing.title || existing.url}</span>
                <span className="mono ellipsis">
                  {t("{0} · 추가 {1}", hostOf(existing.url), dateLabel(existing.createdAt))}
                </span>
                <span className="tags ellipsis">{existing.tags.map((t) => `#${t}`).join("  ")}</span>
              </div>
              <Stars value={existing.rating} />
            </div>
            <p className="hint">{t("같은 주소는 하나만 저장됩니다. 태그나 메모를 바꾸려면 기존 북마크를 수정하세요.")}</p>
            <div className="button-row end">
              <button type="button" className="button" onClick={onClose}>
                {t("취소")}
              </button>
              <button type="button" className="button primary" onClick={() => onEditExisting(existing)}>
                <Icon name="open" size={16} />
                {t("기존 북마크 수정")}
              </button>
            </div>
          </div>
        ) : (
          <>
            <label className="field">
              {t("제목")}
              <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} />
            </label>
            <label className="field">
              {t("태그")}
              <input className="input" value={tags} onChange={(e) => setTags(e.target.value)} placeholder={t("개발 Rust 읽을거리")} />
            </label>
            <label className="field">
              {t("메모")}
              <textarea className="input" rows={3} value={note} onChange={(e) => setNote(e.target.value)} />
            </label>
            <div className="field">
              {t("별점")}
              <Stars value={rating} onChange={setRating} size={26} />
            </div>
            {error && <p className="hint error">{error}</p>}
            <div className="button-row end">
              <button type="button" className="button" onClick={onClose}>
                {t("취소")}
              </button>
              <button type="submit" className="button primary" disabled={busy}>
                {t("저장")}
              </button>
            </div>
          </>
        )}
      </form>
    </div>
  );
}
