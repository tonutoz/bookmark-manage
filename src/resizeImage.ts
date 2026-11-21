import { t } from "./i18n";
// 편집 패널의 큰 썸네일이 고해상도 화면에서도 흐리지 않도록 여유 있게 둔다.
const MAX_WIDTH = 800;
const MAX_HEIGHT = 1200;

/**
 * 파일·붙여넣기·URL 어디서 온 이미지든 같은 경로로 줄인다.
 * 최대 800x1200 에 맞춰 JPEG 로 다시 인코딩한 바이트를 돌려준다 (투명 배경은 흰색).
 */
export async function toThumbnail(source: Blob): Promise<number[]> {
  const src = URL.createObjectURL(source);
  try {
    const img = new Image();
    img.src = src;
    try {
      await img.decode();
    } catch {
      throw new Error(t("이미지 파일이 아니거나 읽을 수 없는 형식입니다"));
    }
    const scale = Math.min(1, MAX_WIDTH / img.naturalWidth, MAX_HEIGHT / img.naturalHeight);
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(img.naturalWidth * scale));
    canvas.height = Math.max(1, Math.round(img.naturalHeight * scale));
    const ctx = canvas.getContext("2d")!;
    ctx.fillStyle = "#fff";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
    const out = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/jpeg", 0.82));
    if (!out) throw new Error(t("이미지를 변환하지 못했습니다"));
    return Array.from(new Uint8Array(await out.arrayBuffer()));
  } finally {
    URL.revokeObjectURL(src);
  }
}
