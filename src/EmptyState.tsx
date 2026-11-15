import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BrowserSource, importFrom, pickImportFile } from "./Settings";
import { t, errorText } from "./i18n";

/** 북마크가 하나도 없을 때의 첫 화면: 찾은 브라우저에서 바로 가져오도록 안내한다. */
export function EmptyState({
  onImported,
  onAdd,
  onConnectExtension,
}: {
  onImported: () => void;
  onAdd: () => void;
  onConnectExtension: () => void;
}) {
  const [sources, setSources] = useState<BrowserSource[]>([]);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");

  useEffect(() => {
    invoke<BrowserSource[]>("detect_browsers").then(setSources).catch((e) => setStatus(errorText(e)));
  }, []);

  async function start(label: string, path: string, source: string) {
    setBusy(true);
    setStatus(t("{0} 가져오는 중…", label));
    try {
      setStatus(`${label}: ${await importFrom(path, source)}`);
      onImported();
    } catch (e) {
      setStatus(`${label}: ${errorText(e)}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="empty">
      <div className="empty-head">
        <h1>{t("쓰던 북마크부터 가져오세요")}</h1>
        <p>
          {sources.length > 0
            ? t("이 PC에서 브라우저 프로필 {0}개를 찾았습니다. 폴더는 태그로 바뀌고, 겹치는 주소는 하나로 합쳐집니다.", sources.length)
            : t("브라우저에서 내보낸 북마크 파일을 가져올 수 있습니다. 폴더는 태그로 바뀌고, 겹치는 주소는 하나로 합쳐집니다.")}
        </p>
      </div>

      {sources.length > 0 && (
        <div className="browser-cards">
          {sources.map((s, index) => {
            const [name, profile] = s.label.replace(/\)$/, "").split(" (");
            return (
              <div key={s.path} className="browser-card">
                <div className={`browser-mark placeholder-${index % 6}`} aria-hidden="true">
                  {name.charAt(0)}
                </div>
                <div className="browser-name">{name}</div>
                <div className="mono ellipsis">{profile}</div>
                <button type="button" className="button primary" disabled={busy} onClick={() => start(s.label, s.path, s.source)}>
                  {t("가져오기")}
                </button>
              </div>
            );
          })}
        </div>
      )}

      <div className="button-row center">
        <button
          type="button"
          className="button"
          disabled={busy}
          onClick={async () => {
            const path = await pickImportFile();
            if (path) await start(t("파일"), path, "file");
          }}
        >
          {t("파일에서 가져오기")}
        </button>
        <button type="button" className="button" onClick={onAdd}>
          {t("직접 추가")}
        </button>
      </div>
      {status && <p className="status">{status}</p>}

      <div className="empty-extension">
        <div>
          <div className="title">{t("브라우저에서 바로 저장하려면")}</div>
          <p>{t("확장 프로그램을 연결하면 보고 있는 페이지를 한 번에 저장합니다. 앱이 꺼져 있어도 보관했다가 전송합니다.")}</p>
        </div>
        <button type="button" className="button" onClick={onConnectExtension}>
          {t("확장 연결")}
        </button>
      </div>
    </div>
  );
}
