import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** system 은 윈도우·맥의 밝게/어둡게 설정을 따른다. */
export type Theme = "system" | "light" | "dark";

// index.html 의 첫 줄 스크립트도 같은 키를 읽어, 화면이 그려지기 전에 색을 정한다.
const KEY = "bookmark-manage.theme";
const systemDark = () => window.matchMedia("(prefers-color-scheme: dark)");

function storedTheme(): Theme {
  try {
    const value = localStorage.getItem(KEY);
    return value === "light" || value === "dark" ? value : "system";
  } catch {
    return "system";
  }
}

function apply(theme: Theme) {
  const dark = theme === "dark" || (theme === "system" && systemDark().matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  // 창 제목 표시줄도 같은 색을 따르게 한다. 앱 밖(브라우저 미리보기)에서는 조용히 넘어간다.
  try {
    getCurrentWindow()
      .setTheme(theme === "system" ? null : theme)
      .catch(() => {});
  } catch {
    // 창 API 가 없는 환경
  }
}

export function useTheme(): [Theme, (theme: Theme) => void] {
  const [theme, setTheme] = useState<Theme>(storedTheme);

  useEffect(() => {
    apply(theme);
    try {
      localStorage.setItem(KEY, theme);
    } catch {
      // 저장하지 못해도 이번 실행에서는 그대로 쓴다.
    }
    if (theme !== "system") return;
    // 시스템 설정이 바뀌면 바로 따라간다.
    const media = systemDark();
    const onChange = () => apply("system");
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, [theme]);

  return [theme, setTheme];
}
