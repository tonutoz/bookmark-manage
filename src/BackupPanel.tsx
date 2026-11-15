import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirm, open } from "@tauri-apps/plugin-dialog";
import { t, errorText, locale } from "./i18n";

type BackupFile = { name: string; path: string; size: number; modified: number };
type BackupStatus = {
  dir: string | null;
  lastBackup: number | null;
  keep: number;
  suggestions: string[];
  backups: BackupFile[];
};
type RestoreOutcome = { restored: number | null; needKey: boolean };

const when = (secs: number) => new Date(secs * 1000).toLocaleString(locale);
const size = (bytes: number) =>
  bytes < 1024 * 1024 ? `${Math.ceil(bytes / 1024)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;

/** 백업 폴더 설정, 수동 백업, 복구 키, 복원. */
export function BackupPanel() {
  const [status, setStatus] = useState<BackupStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  // 이 PC의 키로 열리지 않는 백업을 골랐을 때, 복구 키를 물어볼 대상
  const [needKeyFor, setNeedKeyFor] = useState<string | null>(null);
  const [key, setKey] = useState("");

  const refresh = useCallback(
    () => invoke<BackupStatus>("backup_status").then(setStatus).catch((e) => setMessage(errorText(e))),
    [],
  );
  useEffect(() => {
    refresh();
  }, [refresh]);

  async function run(job: () => Promise<string>) {
    setBusy(true);
    try {
      setMessage(await job());
    } catch (e) {
      setMessage(errorText(e));
    } finally {
      setBusy(false);
      refresh();
    }
  }

  const useDir = (dir: string) =>
    run(async () => {
      await invoke("set_backup_dir", { dir });
      return t("백업 폴더를 {0} 로 정했습니다.", dir);
    });

  async function pickDir() {
    const dir = await open({ directory: true, title: t("백업 폴더 선택") });
    if (typeof dir === "string") await useDir(dir);
  }

  async function restore(path: string, withKey?: string) {
    if (!withKey) {
      const ok = await confirm(
        t("현재 북마크 전체를 이 백업의 내용으로 바꿉니다. 병합이 아니라 교체입니다.\n지금 데이터는 앱 폴더에 안전 사본으로 남깁니다. 계속할까요?"),
        { title: t("백업에서 복원"), kind: "warning" },
      );
      if (!ok) return;
    }
    await run(async () => {
      const result = await invoke<RestoreOutcome>("restore_backup", { path, key: withKey ?? null });
      if (result.needKey) {
        setNeedKeyFor(path);
        return withKey
          ? t("복구 키가 맞지 않습니다. 백업을 만든 PC의 복구 키를 다시 확인하세요.")
          : t("이 PC의 키로는 열 수 없는 백업입니다. 백업을 만든 PC의 복구 키를 입력하세요.");
      }
      setNeedKeyFor(null);
      setKey("");
      return t("복원했습니다. 북마크 {0}개.", result.restored);
    });
  }

  async function pickFile() {
    const path = await open({
      multiple: false,
      title: t("백업 파일 선택"),
      filters: [{ name: t("백업 파일"), extensions: ["bmbackup"] }],
    });
    if (typeof path === "string") await restore(path);
  }

  if (!status) return null;
  return (
    <section className="import">
      <h2>{t("백업")}</h2>
      <p className="hint">
        {status.dir
          ? t("백업 폴더: {0} · 마지막 백업: {1}", status.dir, status.lastBackup ? when(status.lastBackup) : t("없음"))
          : t("백업 폴더가 아직 없습니다. 구글 드라이브 안의 폴더를 고르면 백업이 드라이브로 올라갑니다.")}
      </p>
      <div className="import-sources">
        {status.suggestions
          .filter((dir) => dir !== status.dir)
          .map((dir) => (
            <button key={dir} type="button" disabled={busy} onClick={() => useDir(dir)}>
              {t("구글 드라이브 사용:")} {dir}
            </button>
          ))}
        <button type="button" disabled={busy} onClick={pickDir}>
          {t("폴더 선택…")}
        </button>
        <button
          type="button"
          disabled={busy || !status.dir}
          onClick={() => run(async () => t("백업했습니다: {0}", await invoke<string>("backup_now")))}
        >
          {t("지금 백업")}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() =>
            run(async () => {
              await navigator.clipboard.writeText(await invoke<string>("recovery_key"));
              return t("복구 키를 복사했습니다. 비밀번호 관리자처럼 이 PC 밖의 안전한 곳에 보관하세요.");
            })
          }
        >
          {t("복구 키 복사")}
        </button>
      </div>
      <p className="hint">
        {t("앱이 켜져 있으면 하루에 한 번 자동으로 백업하고 최근 {0}개를 남깁니다. 백업 파일은 암호화되어 있어, 이 PC가 고장 나거나 윈도우를 다시 설치하면 복구 키 없이는 열 수 없습니다. 복구 키를 꼭 따로 보관하세요.", status.keep)}
      </p>

      <h2>{t("복원")}</h2>
      <div className="import-sources">
        {status.backups.map((b) => (
          <button key={b.path} type="button" disabled={busy} title={b.name} onClick={() => restore(b.path)}>
            {when(b.modified)} ({size(b.size)})
          </button>
        ))}
        <button type="button" disabled={busy} onClick={pickFile}>
          {t("파일에서 복원…")}
        </button>
      </div>
      {needKeyFor && (
        <form
          className="import-sources key-form"
          onSubmit={(e) => {
            e.preventDefault();
            restore(needKeyFor, key);
          }}
        >
          <input
            value={key}
            onChange={(e) => setKey(e.target.value)}
            type="password"
            placeholder={t("복구 키 (16진수 64자)")}
            required
          />
          <button type="submit" disabled={busy}>
            {t("이 키로 복원")}
          </button>
        </form>
      )}
      {message && <p className="status">{message}</p>}
    </section>
  );
}
