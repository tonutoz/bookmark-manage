import { useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Icon } from "./Icon";
import { Stars } from "./Stars";
import { ThumbBox } from "./Thumbnail";
import { Bookmark, Layout, dateLabel, hostOf } from "./types";
import { t } from "./i18n";

/** 카드 최소 너비와 카드 사이 간격 (px). 창 너비에 따라 열 수가 정해진다. */
const CARD_MIN = 210;
const GAP = 16;

const tagText = (b: Bookmark) => b.tags.map((t) => `#${t}`).join("  ");

const opensLabel = (b: Bookmark) => (b.openCount > 0 ? t("{0}회 열어봄", b.openCount.toLocaleString()) : "");

type ItemProps = {
  bookmark: Bookmark;
  selected: boolean;
  /** 여러 개를 고르는 중이면 체크 표시를 보여준다. */
  checkable: boolean;
  onSelect: () => void;
  onOpen: () => void;
};

function Check({ on }: { on: boolean }) {
  return (
    <span className="check" aria-hidden="true">
      {on && (
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3.5" strokeLinecap="round" strokeLinejoin="round">
          <path d="M5 12.5 10 17.5 19 7" />
        </svg>
      )}
    </span>
  );
}

/** 편집 패널을 거치지 않고 바로 브라우저로 여는 버튼. */
function OpenButton({ onOpen }: { onOpen: () => void }) {
  return (
    <button type="button" className="icon-button open-button" aria-label={t("브라우저에서 열기")} title={t("브라우저에서 열기")} onClick={onOpen}>
      <Icon name="open" />
    </button>
  );
}

// 카드와 목록 줄은 "눌러서 선택"하는 큰 버튼과 "바로 열기" 버튼이 나란히 있는 구조다.
// 버튼 안에 버튼을 넣을 수 없어서 바깥을 div 로 감싼다.
function Card({ bookmark: b, selected, checkable, onSelect, onOpen }: ItemProps) {
  return (
    <div className={selected ? "card selected" : "card"}>
      <button type="button" className="card-main" aria-pressed={selected} onClick={onSelect} onDoubleClick={onOpen}>
        {checkable && <Check on={selected} />}
        <ThumbBox bookmark={b} className="card-thumb" />
        <span className="card-body">
          <span className="title ellipsis">{b.title || b.url}</span>
          <span className="mono ellipsis">{hostOf(b.url)}</span>
          <span className="tags ellipsis">{tagText(b) || "\u00a0"}</span>
          <span className="card-foot">
            <Stars value={b.rating} />
            <span className="mono">{opensLabel(b)}</span>
          </span>
        </span>
      </button>
      {!checkable && <OpenButton onOpen={onOpen} />}
    </div>
  );
}

function Row({ bookmark: b, selected, checkable, onSelect, onOpen }: ItemProps) {
  return (
    <div className={selected ? "list-row selected" : "list-row"}>
      <button type="button" className="row-select" aria-pressed={selected} onClick={onSelect} onDoubleClick={onOpen}>
        {checkable && <Check on={selected} />}
        <ThumbBox bookmark={b} className="row-thumb" />
        <span className="row-main">
          <span className="title ellipsis">{b.title || b.url}</span>
          <span className="mono ellipsis">{b.url}</span>
        </span>
        <span className="row-tags tags ellipsis">{tagText(b)}</span>
        <Stars value={b.rating} />
        <span className="row-opens mono">{opensLabel(b)}</span>
        <span className="row-date mono">{dateLabel(b.createdAt)}</span>
      </button>
      {!checkable && <OpenButton onOpen={onOpen} />}
    </div>
  );
}

/**
 * 카드 격자 또는 한 줄 목록. 화면에 보이는 줄만 실제로 그리고,
 * 끝에 가까워지면 onNeedMore 로 다음 묶음을 요청한다.
 */
export function BookmarkList({
  items,
  layout,
  selectedIds,
  checkable,
  resetKey,
  onSelect,
  onOpen,
  onNeedMore,
}: {
  items: Bookmark[];
  layout: Layout;
  selectedIds: Set<number>;
  /** 여러 개를 고르는 중 */
  checkable: boolean;
  /** 값이 바뀌면 맨 위로 스크롤한다 (검색·필터·정렬 변경). */
  resetKey: string;
  onSelect: (id: number) => void;
  onOpen: (bookmark: Bookmark) => void;
  onNeedMore: () => void;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const element = scroller.current!;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const columns = layout === "grid" ? Math.max(1, Math.floor((width + GAP) / (CARD_MIN + GAP))) : 1;
  const rowCount = Math.ceil(items.length / columns);

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scroller.current,
    estimateSize: () => (layout === "grid" ? 262 : 73),
    overscan: 4,
  });
  const rows = virtualizer.getVirtualItems();
  const lastRow = rows.length ? rows[rows.length - 1].index : -1;

  // 열 수나 보기 방식이 바뀌면 줄 높이가 달라지므로 다시 잰다.
  useEffect(() => {
    virtualizer.measure();
  }, [columns, layout]);

  useEffect(() => {
    scroller.current?.scrollTo({ top: 0 });
  }, [resetKey]);

  useEffect(() => {
    if ((lastRow + 1) * columns >= items.length - 30) onNeedMore();
  }, [lastRow, columns, items.length, onNeedMore]);

  return (
    <div className="scroller" ref={scroller}>
      <div className="virtual" style={{ height: virtualizer.getTotalSize() }}>
        {rows.map((row) => (
          <div
            key={row.key}
            data-index={row.index}
            ref={virtualizer.measureElement}
            className={layout === "grid" ? "virtual-row grid-row" : "virtual-row"}
            style={{
              transform: `translateY(${row.start}px)`,
              gridTemplateColumns: layout === "grid" ? `repeat(${columns}, minmax(0, 1fr))` : undefined,
            }}
          >
            {items.slice(row.index * columns, (row.index + 1) * columns).map((b) => {
              const props: ItemProps = {
                bookmark: b,
                selected: selectedIds.has(b.id),
                checkable,
                onSelect: () => onSelect(b.id),
                onOpen: () => onOpen(b),
              };
              return layout === "grid" ? <Card key={b.id} {...props} /> : <Row key={b.id} {...props} />;
            })}
          </div>
        ))}
      </div>
    </div>
  );
}
