// Rust(src-tauri/src)와 주고받는 자료 형태. 이름은 Rust 쪽 serde(camelCase)와 맞춘다.

export interface AppInfo {
  name: string;
  version: string;
  os: string;
}

/** 새 파일이 생겼을 때: ask = 바로 물어보기(팝업), manual = 정리함에 모아 두기 */
export type OrganizeMode = "ask" | "manual";

export interface Settings {
  setupDone: boolean;
  rootFolder: string | null;
  watchFolders: string[];
  organizeMode: OrganizeMode;
}

export interface ExampleFolder {
  name: string;
  children: string[];
}

export interface SetupInfo {
  documents: string | null;
  desktop: string | null;
  downloads: string | null;
  rootFolderName: string;
  separator: string;
  teacherExample: ExampleFolder[];
}

/** empty = 빈 정리 폴더, teacher = 교사용 예시 폴더, existing = 쓰던 폴더 그대로 */
export type FolderPreset = "empty" | "teacher" | "existing";

/** 처음 설정에서 고친 폴더 구성 */
export interface FolderSpec {
  name: string;
  children: FolderSpec[];
}

export interface SetupRequest {
  baseDir: string;
  preset: FolderPreset;
  /** preset이 teacher일 때 만들 폴더(없으면 교사용 예시 그대로) */
  structure?: FolderSpec[];
  watchFolders: string[];
  organizeMode: OrganizeMode;
}

export interface FolderEntry {
  name: string;
  path: string;
  fileCount: number;
  folderCount: number;
}

export interface FolderMatch {
  name: string;
  path: string;
  /** 정리 폴더부터의 경로 이름들 */
  trail: string[];
}

/** 팝업에서 물어볼 새 파일 */
export interface PendingFile {
  path: string;
  name: string;
  sourceDir: string;
  size: number;
}

export interface PopupState {
  current: PendingFile | null;
  /** 지금 파일 말고 기다리는 파일 수 */
  remaining: number;
}

export interface PutResult {
  moveId: number;
  movedPath: string;
  destDir: string;
}

export interface InboxItem {
  id: number;
  path: string;
  name: string;
  sourceDir: string;
  addedAt: number;
  size: number;
}

/** 여러 파일 넣기 결과(파일마다) */
export interface PutOutcome {
  path: string;
  moveId: number | null;
  error: string | null;
}

export interface UndoManyResult {
  restored: number;
  errors: string[];
}

export interface TrashResult {
  deletionIds: number[];
  errors: string[];
}
