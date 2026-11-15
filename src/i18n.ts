import { TRANSLATIONS } from "./locales";

/**
 * 다국어. 코드에는 한국어 문구를 그대로 쓰고 t() 로 감싼다.
 * 한국어 문구가 곧 번역표의 열쇠라서, 번역이 빠진 문구는 한국어로 보인다.
 *
 * 언어는 앱이 뜰 때 한 번 정해지고, 바꾸면 화면을 다시 불러온다.
 * 그래서 파일 맨 위의 상수에서 t() 를 써도 된다.
 */
export type Lang = "ko" | "en" | "ja";
/** system 은 윈도우·맥의 언어를 따른다. */
export type LangChoice = "system" | Lang;

const KEY = "bookmark-manage.lang";
const LOCALES: Record<Lang, string> = { ko: "ko-KR", en: "en-US", ja: "ja-JP" };

export function storedLang(): LangChoice {
  try {
    const value = localStorage.getItem(KEY);
    return value === "ko" || value === "en" || value === "ja" ? value : "system";
  } catch {
    return "system";
  }
}

function resolve(choice: LangChoice): Lang {
  if (choice !== "system") return choice;
  const system = (navigator.language || "ko").toLowerCase();
  if (system.startsWith("ja")) return "ja";
  if (system.startsWith("ko")) return "ko";
  return "en";
}

/** 지금 화면에 쓰는 언어. */
export const lang: Lang = resolve(storedLang());
/** 날짜와 숫자 표기에 쓰는 로캘. */
export const locale = LOCALES[lang];

document.documentElement.lang = lang;

/** 언어를 바꾸고 화면을 다시 불러온다. */
export function changeLang(choice: LangChoice) {
  try {
    localStorage.setItem(KEY, choice);
  } catch {
    // 저장하지 못하면 바꿀 수 없다.
    return;
  }
  location.reload();
}

/**
 * 문구 안의 자리 표시를 채운다.
 * {0} 은 그대로 끼워 넣고, {0|bookmark|bookmarks} 는 값이 1 이면 앞의 것, 아니면 뒤의 것을 쓴다.
 */
function fill(text: string, args: unknown[]) {
  return text.replace(/\{(\d+)(?:\|([^|}]*)\|([^}]*))?\}/g, (_, index, one, many) => {
    const value = args[Number(index)];
    if (one === undefined) return String(value ?? "");
    const count = typeof value === "number" ? value : Number(String(value).replace(/[^\d.-]/g, ""));
    return count === 1 ? one : many;
  });
}

/** 한국어 문구를 지금 언어로 바꾼다. 번역이 없으면 한국어 그대로 쓴다. */
export function t(key: string, ...args: unknown[]): string {
  const translated = lang === "ko" ? undefined : TRANSLATIONS[key]?.[lang];
  return fill(translated ?? key, args);
}

/**
 * 문장 가운데에 굵은 글씨 같은 요소를 끼워야 할 때 쓴다.
 * {0} 자리를 기준으로 앞뒤 두 조각을 돌려준다. 언어마다 {0} 의 위치가 달라도 된다.
 */
export function tSplit(key: string, ...rest: unknown[]): [string, string] {
  const marker = "\u0001";
  const [before, after = ""] = t(key, marker, ...rest).split(marker);
  return [before, after];
}

/**
 * 오류를 화면에 보여줄 문구로 바꾼다.
 * 백엔드는 한국어 문구를 돌려주므로, "고정 문구: 자세한 내용" 꼴이면 앞부분만 번역한다.
 */
export function errorText(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  if (lang === "ko" || TRANSLATIONS[message]) return t(message);
  const colon = message.indexOf(": ");
  if (colon > 0) {
    const head = `${message.slice(0, colon)}: {0}`;
    if (TRANSLATIONS[head]) return t(head, message.slice(colon + 2));
  }
  return message;
}
