import { t } from "./i18n";
const POINTS = "8,1.5 10,6 14.8,6.4 11.1,9.6 12.3,14.3 8,11.7 3.7,14.3 4.9,9.6 1.2,6.4 6,6";

function Star({ on, size }: { on: boolean; size: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" aria-hidden="true">
      <polygon points={POINTS} fill={on ? "var(--star)" : "var(--star-off)"} />
    </svg>
  );
}

/** 별점. onChange 를 주면 누를 수 있고, 같은 별을 다시 누르면 평가를 지운다. */
export function Stars({
  value,
  onChange,
  size = 15,
}: {
  value: number | null;
  onChange?: (value: number | null) => void;
  size?: number;
}) {
  const label = value ? t("별점 {0}점", value) : t("별점 없음");
  if (!onChange) {
    return (
      <span className="stars" role="img" aria-label={label}>
        {[1, 2, 3, 4, 5].map((n) => (
          <Star key={n} on={n <= (value ?? 0)} size={size} />
        ))}
      </span>
    );
  }
  return (
    <span className="stars">
      {[1, 2, 3, 4, 5].map((n) => (
        <button
          key={n}
          type="button"
          className="star-button"
          aria-label={t("별점 {0}점", n)}
          aria-pressed={value === n}
          onClick={() => onChange(value === n ? null : n)}
        >
          <Star on={n <= (value ?? 0)} size={size} />
        </button>
      ))}
    </span>
  );
}
