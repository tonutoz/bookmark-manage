/** 앱에서 쓰는 선 아이콘 모음. 24x24 기준, 선 굵기 2. */
const PATHS = {
  open: "M14 5h5v5M19 5l-8 8M11 7H6a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1v-5",
  trash: "M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3",
  close: "M6 6l12 12M18 6 6 18",
  plus: "M12 5v14M5 12h14",
  select: "M5 4h14a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1zM8 12.5l3 3 5-6",
  image: "M5 5h14a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zM4 16l4.5-4.5 3.5 3.5 3-3 5 5M15 9.5h.01",
  save: "M5 12.5 10 17.5 19 7",
  upload: "M12 16V5M7.5 9.5 12 5l4.5 4.5M5 19h14",
  download: "M12 5v11M7.5 11.5 12 16l4.5-4.5M5 19h14",
  dashboard: "M4 13h6V4H4zM14 20h6v-9h-6zM4 20h6v-3H4zM14 7h6V4h-6z",
  chevron: "M6 9l6 6 6-6",
  edit: "M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4",
} as const;

export type IconName = keyof typeof PATHS;

/** 장식용 아이콘. 버튼에 넣을 때는 버튼 쪽에 aria-label 과 title 을 붙인다. */
export function Icon({ name, size = 18 }: { name: IconName; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
