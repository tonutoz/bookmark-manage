import { FormEvent, useState } from "react";
import { t, tSplit } from "./i18n";

export type DeleteRequest = {
  ids: number[];
  /** 무엇을 지우는지: "spam.example", "#개발", "선택한 북마크" 등 */
  target: string;
  /** 사용자가 그대로 입력해야 삭제 버튼이 켜지는 문구. 사이트를 지울 때는 그 도메인이다. */
  phrase: string;
  /** 지워질 북마크 제목 몇 개 (미리 보기) */
  samples: string[];
};

/**
 * 여러 북마크를 한 번에 지우기 전의 확인 창.
 * 버튼만 누르면 되는 확인은 습관적으로 넘기기 쉬워서, 문구를 직접 입력해야 삭제할 수 있게 한다.
 */
export function ConfirmDelete({
  request,
  onConfirm,
  onClose,
}: {
  request: DeleteRequest;
  onConfirm: (ids: number[]) => void;
  onClose: () => void;
}) {
  const [typed, setTyped] = useState("");
  const count = request.ids.length;
  const matches = typed.trim().toLowerCase() === request.phrase.toLowerCase();
  const more = count - request.samples.length;
  // 굵게 보여줄 말(대상, 입력할 문구)의 앞뒤 문장
  const about = tSplit("{0} 에 해당하는 북마크 {1}개를 모두 지웁니다. 별점, 메모, 썸네일도 함께 사라집니다.", count.toLocaleString());
  const typeIt = tSplit("계속하려면 {0} 를 입력하세요");

  function submit(e: FormEvent) {
    e.preventDefault();
    if (matches) onConfirm(request.ids);
  }

  return (
    <div
      className="overlay"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <form className="dialog" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" onSubmit={submit}>
        <h2 id="confirm-title">{t("북마크 {0}개 삭제", count.toLocaleString())}</h2>
        <p>
          {about[0]}
          <strong>{request.target}</strong>
          {about[1]}
        </p>

        {request.samples.length > 0 && (
          <ul className="sample-list">
            {request.samples.map((title, index) => (
              <li key={index} className="ellipsis">
                {title}
              </li>
            ))}
            {more > 0 && <li className="more">{t("외 {0}개", more.toLocaleString())}</li>}
          </ul>
        )}

        <label className="field">
          <span>
            {typeIt[0]}
            <strong className="phrase">{request.phrase}</strong>
            {typeIt[1]}
          </span>
          <input
            className="input mono-input"
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            autoFocus
            autoComplete="off"
            spellCheck={false}
          />
        </label>

        <div className="button-row end">
          <button type="button" className="button" onClick={onClose}>
            {t("취소")}
          </button>
          <button type="submit" className="button danger-fill" disabled={!matches}>
            {t("{0}개 삭제", count.toLocaleString())}
          </button>
        </div>
      </form>
    </div>
  );
}
