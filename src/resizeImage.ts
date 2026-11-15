import { t } from "./i18n";
const MAX_WIDTH = 400;
const MAX_HEIGHT = 600;

/**
 * 파일·붙여넣기·URL 어디서 온 이미지든 같은 경로로 줄인다.
 * 최대 400x600 에 맞춰 JPEG 로 다시 인코딩한 바이트를 돌려준다 (투명 배경은 흰색).
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
