import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { BackupPanel } from "./BackupPanel";
import type { Theme } from "./theme";
import { t, errorText, LangChoice, changeLang, storedLang } from "./i18n";

export type SettingsTab = "import" | "export" | "extension" | "backup" | "display";
export type BrowserSource = { label: string; path: string; source: string };
type ImportSummary = { total: number; added: number; merged: number; skipped: number };
type ExtensionInfo = { port: number; token: string; error: string | null };

const TABS: { key: SettingsTab; label: string }[] = [
  { key: "import", label: t("가져오기") },
  { key: "export", label: t("내보내기") },
  { key: "extension", label: t("브라우저 확장") },
  { key: "backup", label: t("백업") },
  { key: "display", label: t("화면") },
];

const THEMES: { key: Theme; label: string }[] = [
  { key: "system", label: t("시스템 설정") },
  { key: "light", label: t("일반") },
  { key: "dark", label: t("다크") },
];

const EXPORTS = {
  html: { label: t("HTML로 내보내기"), name: t("북마크 HTML"), file: "bookmarks.html" },
  json: { label: t("JSON 백업 내보내기"), name: t("JSON 백업"), file: "bookmarks-backup.json" },
} as const;

/** 한 번에 하나만 실행되는 작업과 그 결과 문구. */
function useJob() {
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  async function run(label: string, job: () => Promise<string>) {
    setBusy(true);
    setStatus(t("{0} 처리 중…", label));
    try {
      setStatus(`${label}: ${await job()}`);
    } catch (e) {
      setStatus(`${label}: ${errorText(e)}`);
    } finally {
      setBusy(false);
    }
  }
  return { busy, status, setStatus, run };
}

/** 가져오기 한 건을 실행하고 결과 문구를 만든다. 첫 실행 화면에서도 쓴다. */
export async function importFrom(path: string, source: string) {
  const s = await invoke<ImportSummary>("import_file", { path, source });
  return (
    t("총 {0}개 중 {1}개 추가, {2}개는 이미 있어 태그만 병합", s.total, s.added, s.merged) +
    (s.skipped ? t(", {0}개는 저장할 수 없는 주소라 건너뜀", s.skipped) : "")
  );
}

export async function pickImportFile() {
  const path = await open({
    multiple: false,
    title: t("북마크 파일 선택"),
    filters: [
      { name: t("북마크 파일"), extensions: ["html", "htm", "json", "sqlite"] },
      { name: t("모든 파일"), extensions: ["*"] },
    ],
  });
  return typeof path === "string" ? path : null;
}

function ImportTab({ onImported }: { onImported: () => void }) {
  const [sources, setSources] = useState<BrowserSource[]>([]);
  const { busy, status, setStatus, run } = useJob();

  useEffect(() => {
    invoke<BrowserSource[]>("detect_browsers").then(setSources).catch((e) => setStatus(errorText(e)));
  }, []);

  const start = (label: string, path: string, source: string) =>
    run(label, async () => {
      const result = await importFrom(path, source);
      onImported();
      return result;
    });

  return (
    <section className="panel">
      <h2>{t("브라우저에서 가져오기")}</h2>
      <div className="button-row">
        {sources.map((s) => (
          <button key={s.path} type="button" className="button" disabled={busy} title={s.path} onClick={() => start(s.label, s.path, s.source)}>
            {s.label}
          </button>
        ))}
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
      </div>
      <p className="hint">
        {t("폴더는 태그로 바뀌고, 이미 저장된 주소는 태그만 합쳐집니다. 파일은 북마크 HTML, 크롬 Bookmarks, 파이어폭스와 토르의 places.sqlite, 이 앱의 JSON 백업을 지원합니다.")}
      </p>
      {status && <p className="status">{status}</p>}
    </section>
  );
}

function ExportTab({ filter, filtered }: { filter: unknown; filtered: boolean }) {
  const { busy, status, run } = useJob();

  async function exportTo(format: keyof typeof EXPORTS) {
    const { name, file } = EXPORTS[format];
    const path = await save({ title: name, defaultPath: file, filters: [{ name, extensions: [format] }] });
    if (!path) return;
    await run(name, async () => {
      const count = await invoke<number>("export_file", { path, format, filter });
      return t("{0}개를 {1} 에 저장했습니다", count, path);
    });
  }

  return (
    <section className="panel">
      <h2>{t("내보내기")}</h2>
      <p className="hint">
        {filtered
          ? t("지금 북마크 화면에 걸려 있는 검색과 필터 결과만 내보냅니다.")
          : t("북마크 전체를 내보냅니다. 일부만 내보내려면 북마크 화면에서 검색하거나 태그를 고른 뒤 다시 오세요.")}
      </p>
      <div className="button-row">
        {(Object.keys(EXPORTS) as (keyof typeof EXPORTS)[]).map((format) => (
          <button key={format} type="button" className="button" disabled={busy} onClick={() => exportTo(format)}>
            {EXPORTS[format].label}
          </button>
        ))}
      </div>
      <p className="hint">
        {t("HTML은 브라우저로 다시 넣는 용도입니다. 첫 번째 태그를 폴더로 삼고 별점, 메모, 썸네일은 담지 않습니다. JSON은 전부 담지만 암호화되지 않은 파일이니 보관 위치에 주의하세요.")}
      </p>
      {status && <p className="status">{status}</p>}
    </section>
  );
}

function ExtensionTab() {
  const [info, setInfo] = useState<ExtensionInfo | null>(null);
  const [status, setStatus] = useState("");

  useEffect(() => {
    invoke<ExtensionInfo>("extension_info").then(setInfo).catch((e) => setStatus(errorText(e)));
  }, []);

  return (
    <section className="panel">
      <h2>{t("브라우저 확장 연결")}</h2>
      {info?.error ? (
        <p className="hint error">{t("확장 수신 서버를 켜지 못했습니다:")} {info.error}</p>
      ) : (
        <>
          <ol className="steps">
            <li>{t("크롬은 chrome://extensions 에서 개발자 모드를 켜고, 이 프로그램의 extension 폴더를 불러옵니다.")}</li>
            <li>{t("아래 버튼으로 연결 토큰을 복사합니다.")}</li>
            <li>{t("확장 아이콘을 눌러 연결 설정에 붙여넣고 연결 확인을 누릅니다.")}</li>
          </ol>
          <div className="button-row">
            <button
              type="button"
              className="button primary"
              disabled={!info}
              onClick={async () => {
                await navigator.clipboard.writeText(info!.token);
                setStatus(t("연결 토큰을 복사했습니다."));
              }}
            >
              {t("연결 토큰 복사")}
            </button>
          </div>
          <p className="hint">
            {t("확장 프로그램은 이 PC의 127.0.0.1:{0} 로만 보내며, 토큰이 있어야 저장됩니다. 앱이 꺼져 있으면 확장이 보관했다가 앱이 켜질 때 전송합니다.", info?.port ?? "…")}
          </p>
        </>
      )}
      {status && <p className="status">{status}</p>}
    </section>
  );
}

/** 언어 이름은 번역하지 않고 그 언어로 적는다. */
const LANGS: { key: LangChoice; label: string }[] = [
  { key: "system", label: t("시스템 언어") },
  { key: "ko", label: "한국어" },
  { key: "en", label: "English" },
  { key: "ja", label: "日本語" },
];

function DisplayTab({ theme, onTheme }: { theme: Theme; onTheme: (theme: Theme) => void }) {
  return (
    <>
    <section className="panel">
      <h2 id="lang-label">{t("언어")}</h2>
      <div className="choices" role="radiogroup" aria-labelledby="lang-label">
        {LANGS.map((l) => (
          <button key={l.key} type="button" role="radio" aria-checked={storedLang() === l.key} onClick={() => changeLang(l.key)}>
            {l.label}
          </button>
        ))}
      </div>
      <p className="hint">
        {t("언어를 바꾸면 화면을 다시 불러옵니다. 저장한 북마크의 제목, 태그, 메모는 바뀌지 않습니다. 브라우저 확장 팝업은 브라우저의 언어를 따릅니다.")}
      </p>
    </section>
    <section className="panel">
      <h2 id="theme-label">{t("화면 모드")}</h2>
      <div className="choices" role="radiogroup" aria-labelledby="theme-label">
        {THEMES.map((t) => (
          <button key={t.key} type="button" role="radio" aria-checked={theme === t.key} onClick={() => onTheme(t.key)}>
            {t.label}
          </button>
        ))}
      </div>
      <p className="hint">
        {t("시스템 설정을 고르면 윈도우나 맥의 밝게, 어둡게 설정을 따라 자동으로 바뀝니다. 브라우저 확장 팝업은 항상 시스템 설정을 따릅니다.")}
      </p>
    </section>
    </>
  );
}

export function Settings({
  tab,
  onTab,
  exportFilter,
  exportFiltered,
  onImported,
  theme,
  onTheme,
}: {
  tab: SettingsTab;
  onTab: (tab: SettingsTab) => void;
  exportFilter: unknown;
  exportFiltered: boolean;
  onImported: () => void;
  theme: Theme;
  onTheme: (theme: Theme) => void;
}) {
  return (
    <main className="content settings">
      <h1>{t("설정")}</h1>
      <div className="tabs" role="tablist">
        {TABS.map((t) => (
          <button
            key={t.key}
            type="button"
            role="tab"
            aria-selected={tab === t.key}
            className={tab === t.key ? "tab active" : "tab"}
            onClick={() => onTab(t.key)}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div className="settings-body">
        {tab === "import" && <ImportTab onImported={onImported} />}
        {tab === "export" && <ExportTab filter={exportFilter} filtered={exportFiltered} />}
        {tab === "extension" && <ExtensionTab />}
        {tab === "backup" && <BackupPanel />}
        {tab === "display" && <DisplayTab theme={theme} onTheme={onTheme} />}
      </div>
    </main>
  );
}
