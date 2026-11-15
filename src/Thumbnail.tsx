import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toThumbnail } from "./resizeImage";
import { Bookmark, hostOf } from "./types";
import { t, errorText } from "./i18n";

/** 썸네일이 없을 때 쓰는 색 블록. 북마크마다 고정된 색이 나오도록 id 로 고른다. */
const PLACEHOLDERS = 6;

/** 썸네일 영역. 등록된 이미지가 있으면 그것을, 없으면 색 블록과 사이트 머리글자를 보여준다. */
export function ThumbBox({ bookmark, className }: { bookmark: Bookmark; className: string }) {
  const [src, setSrc] = useState<string | null>(null);
  const { id, thumbVersion } = bookmark;

  useEffect(() => {
    if (thumbVersion === null) {
      setSrc(null);
      return;
    }
    let url: string | null = null;
    let cancelled = false;
    invoke<ArrayBuffer>("get_thumbnail", { id })
      .then((data) => {
        if (cancelled) return;
        url = URL.createObjectURL(new Blob([data], { type: "image/jpeg" }));
        setSrc(url);
      })
      .catch(() => setSrc(null));
    return () => {
      cancelled = true;
      if (url) URL.revokeObjectURL(url);
    };
  }, [id, thumbVersion]);

  if (src) {
    return (
      <div className={`thumb ${className}`}>
        <img src={src} alt="" />
      </div>
    );
  }
  return (
    <div className={`thumb placeholder-${id % PLACEHOLDERS} ${className}`} aria-hidden="true">
      {hostOf(bookmark.url).charAt(0).toUpperCase()}
    </div>
  );
}

/** 썸네일 등록: 파일 선택, 이미지 URL, 클립보드 붙여넣기. */
export function ThumbEditor({ bookmark, onChange }: { bookmark: Bookmark; onChange: (b: Bookmark) => void }) {
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const fileInput = useRef<HTMLInputElement>(null);
  const id = bookmark.id;

  async function run(job: () => Promise<Bookmark>) {
    setBusy(true);
    setError("");
    try {
      onChange(await job());
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  const save = (image: Blob) =>
    run(async () => invoke<Bookmark>("set_thumbnail", { id, data: await toThumbnail(image) }));

  // 패널이 열려 있는 동안 Ctrl+V 로 이미지를 붙여넣을 수 있다. 텍스트 붙여넣기는 건드리지 않는다.
  useEffect(() => {
    function onPaste(e: ClipboardEvent) {
      const file = Array.from(e.clipboardData?.files ?? []).find((f) => f.type.startsWith("image/"));
      if (!file) return;
      e.preventDefault();
      save(file);
    }
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [id]);

  return (
    <div className="thumb-editor">
      <input
        ref={fileInput}
        type="file"
        accept="image/*"
        hidden
        onChange={(e) => {
          const file = e.target.files?.[0];
          e.target.value = "";
          if (file) save(file);
        }}
      />
      <div className="button-row">
        <button type="button" className="button" disabled={busy} onClick={() => fileInput.current?.click()}>
          {t("파일 선택")}
        </button>
        {bookmark.thumbVersion !== null && (
          <button
            type="button"
            className="button"
            disabled={busy}
            onClick={() => run(() => invoke<Bookmark>("remove_thumbnail", { id }))}
          >
            {t("썸네일 삭제")}
          </button>
        )}
      </div>
      <form
        className="button-row"
        onSubmit={(e) => {
          e.preventDefault();
          run(async () => {
            const data = await invoke<ArrayBuffer>("fetch_image", { url });
            const updated = await invoke<Bookmark>("set_thumbnail", { id, data: await toThumbnail(new Blob([data])) });
            setUrl("");
            return updated;
          });
        }}
      >
        <input
          className="input grow"
          aria-label={t("이미지 URL")}
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder={t("이미지 URL")}
          required
        />
        <button type="submit" className="button" disabled={busy}>
          {t("가져오기")}
        </button>
      </form>
      <p className={error ? "hint error" : "hint"}>
        {busy ? t("처리 중…") : error || t("이미지를 복사한 뒤 Ctrl+V 로도 등록됩니다.")}
      </p>
    </div>
  );
}
