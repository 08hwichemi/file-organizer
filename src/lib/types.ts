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

export interface SetupRequest {
  baseDir: string;
  preset: FolderPreset;
  watchFolders: string[];
  organizeMode: OrganizeMode;
}

export interface FolderEntry {
  name: string;
  path: string;
  fileCount: number;
  folderCount: number;
}
