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

/** 앱이 켜질 때 받은 다운로드·바탕화면·문서 위치(정리 폴더 이름을 쉽게 보여 줄 때 씀) */
let knownFolders: KnownFolders | null = null;

export function setKnownFolders(known: KnownFolders | null) {
  knownFolders = known;
}

/** 정리 폴더를 부르는 이름: 보통 폴더 이름, 바탕화면을 그대로 쓰면 "바탕화면"(경로 "바탕화면 › 업무") */
export function rootLabel(root: string): string {
  return friendlyName(root, knownFolders);
}

export interface Crumb {
  name: string;
  path: string;
}

/** 정리 폴더(root)부터 path까지의 경로 조각들. path가 root 밖이면 null. */
export function trailFor(root: string, path: string, separator: string): Crumb[] | null {
  const rootName = rootLabel(root);
  const base = root.replace(/[\\/]+$/, "");
  if (samePath(base, path)) return [{ name: rootName, path: base }];
  const prefix = base + separator;
  if (!path.toLowerCase().startsWith(prefix.toLowerCase())) return null;
  const crumbs: Crumb[] = [{ name: rootName, path: base }];
  let current = base;
  for (const part of path.slice(prefix.length).split(separator).filter(Boolean)) {
    current = current + separator + part;
    crumbs.push({ name: part, path: current });
  }
  return crumbs;
}

/** 넣을 곳 표시용: "정리 폴더 › 평가 › 수행평가". 정리 폴더 밖이면 전체 경로. */
export function displayPath(root: string | null, path: string, separator: string): string {
  const trail = root ? trailFor(root, path, separator) : null;
  return trail ? trail.map((c) => c.name).join(" › ") : path;
}

export function parentPath(path: string, separator: string): string {
  const i = path.replace(/[\\/]+$/, "").lastIndexOf(separator);
  return i > 0 ? path.slice(0, i) : path;
}
