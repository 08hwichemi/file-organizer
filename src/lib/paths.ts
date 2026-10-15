// 경로 문자열 도우미. 실제 경로 확인은 Rust가 하고, 여기서는 보여 주기용으로만 쓴다.

export function baseName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const parts = trimmed.split(/[\\/]/);
  return parts[parts.length - 1] || trimmed;
}

export function joinPath(base: string, name: string, separator: string): string {
  return base.replace(/[\\/]+$/, "") + separator + name;
}

/** Windows 경로는 대소문자를 가리지 않으므로 소문자로 비교한다. */
export function samePath(a: string, b: string): boolean {
  const norm = (p: string) => p.replace(/[\\/]+$/, "").replace(/\//g, "\\").toLowerCase();
  return norm(a) === norm(b);
}

export interface KnownFolders {
  downloads: string | null;
  desktop: string | null;
  documents: string | null;
}

/** 지켜볼 폴더 등에 붙일 쉬운 이름: 다운로드·바탕화면·문서, 그 밖은 폴더 이름. */
export function friendlyName(path: string, known: KnownFolders | null): string {
  if (known?.downloads && samePath(path, known.downloads)) return "다운로드";
  if (known?.desktop && samePath(path, known.desktop)) return "바탕화면";
  if (known?.documents && samePath(path, known.documents)) return "문서";
  return baseName(path);
}
