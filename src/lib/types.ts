// Rust(src-tauri/src)와 주고받는 자료 형태. 이름은 Rust 쪽 serde(camelCase)와 맞춘다.

export interface AppInfo {
  name: string;
  version: string;
  os: string;
}

/** 새 파일이 생겼을 때: ask = 바로 물어보기(팝업), manual = 정리함에 모아 두기 */
export type OrganizeMode = "ask" | "manual";

/**
 * 정리 방식: suggest = 추천만(팝업에서 골라 둠), auto = 바로 정리(규칙에 맞으면 묻지 않고 옮기고 알림),
 * classify = 자동 분류(규칙이 없어도 이름·형식으로 나누고, 모르는 파일은 정리함에)
 */
export type PlaceMode = "suggest" | "auto" | "classify";

export interface Settings {
  setupDone: boolean;
  rootFolder: string | null;
  watchFolders: string[];
  organizeMode: OrganizeMode;
  placeMode: PlaceMode;
  /** 1학기·2학기가 시작하는 달(기본 3·9) */
  firstTermMonth: number;
  secondTermMonth: number;
  /** 컴퓨터를 켜면 자동 실행(트레이로만) */
  autostart: boolean;
  /** 추천만일 때 팝업에 응답이 없으면 n초 뒤 추천 위치에 넣기(0 = 끔) */
  waitSeconds: number;
  /** 자주 쓰는 폴더 1~6 직접 고른 것(null = 자동) */
  pinnedFavorites: (string | null)[];
  /** 파일 검색 단축키("" = 쓰지 않음) */
  searchShortcut: string;
  /** 파일 검색 범위: PC 전체(true, 기본) / 정리 폴더와 지켜볼 폴더만(false) */
  searchWholePc: boolean;
}

/** Everything: ready = PC 전체 검색, notRunning = 설치·실행 안 됨, noDll = SDK 빠짐, unsupported = Windows 아님 */
export type EverythingState = "ready" | "notRunning" | "noDll" | "unsupported";

/** 파일 검색 결과 하나 */
export interface SearchHit {
  path: string;
  name: string;
  dir: string;
  size: number;
  modified: number;
  year: number | null;
  term: number | null;
  class: string | null;
  /** 규칙으로 넣은 파일이면 그 규칙의 단어 */
  rule: string | null;
}

/** 어디서 찾았는지: everything = Everything(PC 전체), pc = 이 프로그램이 읽은 PC 전체, folders = 정리 폴더·지켜볼 폴더 */
export type SearchSource = "everything" | "pc" | "folders";

export interface SearchResponse {
  source: SearchSource;
  everything: EverythingState;
  /** 설정에서 PC 전체를 골랐는지 */
  wantWholePc: boolean;
  /** 파일 목록을 읽는 중인지 */
  indexing: boolean;
  /** 읽은 파일 수(읽는 중이면 지금까지) */
  indexed: number;
  hits: SearchHit[];
}

export interface SearchInfo {
  everything: EverythingState;
  /** 이 프로그램이 읽은 목록이 PC 전체인지 */
  wholePc: boolean;
  indexing: boolean;
  indexed: number;
  shortcutError: string | null;
}

/** 프로그램 상태(트레이와 함께 바뀜) */
export interface Status {
  paused: boolean;
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

/** 정리 규칙(SPEC 4장): 조건 → 저장 위치 */
export interface Rule {
  id: number;
  /** 파일 이름에 들어 있어야 하는 단어 */
  words: string[];
  /** 단어가 여럿일 때 하나라도 있으면 맞음(false = 모두 있어야) */
  anyWord: boolean;
  /** 확장자(소문자, 점 없이). 비면 아무 형식 */
  extensions: string[];
  /** 이 폴더에서 온 파일만 */
  sourceDir: string | null;
  destDir: string;
  /** 연도·학기·학급 폴더를 파일마다 나눌지 */
  split: boolean;
  /** 이 규칙으로 넣은 횟수 */
  used: number;
}

export interface RuleInput {
  words: string[];
  anyWord: boolean;
  extensions: string[];
  sourceDir: string | null;
  destDir: string;
  split: boolean;
}

/** 저장 위치 한 단계. template = 나눌 수 있는 폴더의 자리표시 모양(예: "[연도]") */
export interface DestSegment {
  name: string;
  template: string | null;
}

export interface RuleView extends Rule {
  segments: DestSegment[];
  /** 저장 위치 폴더가 없어졌는지 */
  missing: boolean;
}

/** 넣을 곳을 정한 까닭 */
export type Reason =
  | { kind: "rule"; rule: Rule }
  /** 이름에 정리 폴더의 폴더 이름이 들어 있음 */
  | { kind: "folder"; word: string }
  /** 이름에 자동 분류 단어가 들어 있음 */
  | { kind: "word"; word: string }
  /** 이름으로 몰라서 파일 형식으로 */
  | { kind: "type"; typeName: string };

/** 자동 분류 단어 묶음: 단어 중 하나가 이름에 있으면 folder(정리 폴더 아래 경로)로 */
export interface WordGroup {
  folder: string[];
  words: string[];
}

/** 자동 분류 미리보기: 파일마다 넣을 곳(null이면 정리함에 남음) */
export interface ClassifyPlan {
  path: string;
  name: string;
  target: Suggestion | null;
}

/** 추천 위치 */
export interface Suggestion {
  reason: Reason;
  destDir: string;
  /** 이미 있는 폴더인지(없으면 넣을 때 만든다) */
  exists: boolean;
  /** 지금 있는 가장 깊은 폴더 */
  reveal: string;
}

/** "바로 정리"·"자동 분류"로 옮긴 파일 */
export interface AutoMoved {
  moveId: number;
  name: string;
  destDir: string;
  reason: Reason;
}

export interface PopupState {
  current: PendingFile | null;
  /** 지금 파일 말고 기다리는 파일 수 */
  remaining: number;
  suggestion: Suggestion | null;
  autoMoved: AutoMoved[];
}

/** 넣은 뒤 "다음에도 여기에?" 카드 */
export interface RuleDraft {
  words: string[];
  preselected: string[];
  destDir: string;
  segments: DestSegment[];
  /** 예: "2028년 2학기" */
  exampleLabel: string;
  exampleSegments: string[];
}

/** "규칙을 바꿀까요?" */
export interface RuleChange {
  rule: Rule;
  oldDir: string;
  newDir: string;
}

export interface PutResult {
  moveId: number;
  movedPath: string;
  destDir: string;
  /** 추천 위치에 넣었으면 그 까닭 */
  followed: Reason | null;
  change: RuleChange | null;
  draft: RuleDraft | null;
}

export interface InboxItem {
  id: number;
  path: string;
  name: string;
  sourceDir: string;
  addedAt: number;
  size: number;
  suggestion: Suggestion | null;
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
