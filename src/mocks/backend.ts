// 브라우저 미리보기용 가짜 백엔드. 실제 파일은 건드리지 않고 메모리에서만 흉내 낸다.
// - 주소 뒤 ?mock=ready  : 처음 설정을 건너뛰고 정리함 화면부터
// - 주소 뒤 ?window=popup: 새 파일 팝업 창(새 파일 3개가 기다리는 상태)
//   &auto=1: "바로 정리"로 옮긴 알림부터, &rules=0: 규칙 없이
import type {
  AppInfo,
  AutoMoved,
  DestSegment,
  FolderEntry,
  FolderMatch,
  InboxItem,
  OrganizeMode,
  PendingFile,
  PlaceMode,
  PopupState,
  PutOutcome,
  PutResult,
  Rule,
  RuleDraft,
  RuleInput,
  RuleView,
  Settings,
  Suggestion,
  WordGroup,
  ClassifyPlan,
  EverythingState,
  SearchHit,
  SearchInfo,
  SearchResponse,
  FolderFiles,
  NameAnalysis,
  PieceInfo,
  PieceKind,
  PreviewRow,
  RenameFile,
  RenamePreview,
  RenameRequest,
  RenameResult,
  RosterInfo,
  SearchSource,
  SetupInfo,
  SetupRequest,
  TrashResult,
  UndoManyResult,
} from "../lib/types";

const HOME = "C:\\Users\\선생님";
const SEP = "\\";
const DOWNLOADS = `${HOME}\\Downloads`;
const DESKTOP = `${HOME}\\Desktop`;

const setupInfo: SetupInfo = {
  documents: `${HOME}\\Documents`,
  desktop: DESKTOP,
  downloads: DOWNLOADS,
  rootFolderName: "정리 폴더",
  separator: SEP,
  teacherExample: [
    { name: "업무", children: ["공문", "회의록", "계획서"] },
    { name: "수업", children: [] },
    { name: "평가", children: ["수행평가", "지필고사"] },
    { name: "학급", children: ["상담", "사진", "출결"] },
    { name: "생활기록부", children: [] },
    { name: "연수", children: [] },
    { name: "개인", children: [] },
  ],
};

// ---------- 가짜 폴더 ----------
interface MockFolder {
  files: number;
  children: Record<string, MockFolder>;
}

const folder = (files: number, children: Record<string, MockFolder> = {}): MockFolder => ({
  files,
  children,
});

function teacherTree(): MockFolder {
  return folder(0, {
    업무: folder(4, { 공문: folder(57), 회의록: folder(18), 계획서: folder(11) }),
    수업: folder(2, { "2027 1학기": folder(46), "2027 2학기": folder(12) }),
    평가: folder(0, {
      수행평가: folder(3, { "2027": folder(0, { "1학기": folder(19), "2학기": folder(4) }) }),
      지필고사: folder(22),
    }),
    학급: folder(6, { 상담: folder(8), 사진: folder(120), 출결: folder(15) }),
    생활기록부: folder(31),
    연수: folder(9),
    개인: folder(14),
  });
}

const params = new URLSearchParams(window.location.search);
const startReady = params.get("mock") === "ready" || params.get("window") === "popup";
const readyRoot = `${HOME}\\Documents\\정리 폴더`;

let settings: Settings = startReady
  ? {
      setupDone: true,
      rootFolder: readyRoot,
      watchFolders:
        params.get("many") === "1" ? [DOWNLOADS, DESKTOP, `${HOME}\\Documents`, "D:\\", `${HOME}\\빈 폴더`] : [DOWNLOADS, DESKTOP],
      organizeMode: "ask",
      placeMode: "suggest",
      firstTermMonth: 3,
      secondTermMonth: 9,
      autostart: true,
      waitSeconds: params.get("wait") ? Number(params.get("wait")) : 0,
      pinnedFavorites: [],
      searchShortcut: "Alt+Space",
      searchWholePc: true,
    }
  : {
      setupDone: false,
      rootFolder: null,
      watchFolders: [],
      organizeMode: "ask",
      placeMode: "suggest",
      firstTermMonth: 3,
      secondTermMonth: 9,
      autostart: true,
      waitSeconds: params.get("wait") ? Number(params.get("wait")) : 0,
      pinnedFavorites: [],
      searchShortcut: "Alt+Space",
      searchWholePc: true,
    };

const trees = new Map<string, MockFolder>(startReady ? [[readyRoot, teacherTree()]] : []);

function findFolder(path: string): MockFolder | null {
  for (const [rootPath, tree] of trees) {
    if (path === rootPath) return tree;
    if (!path.startsWith(rootPath + SEP)) continue;
    let node: MockFolder | undefined = tree;
    for (const part of path.slice(rootPath.length + 1).split(SEP)) {
      node = node?.children[part];
    }
    return node ?? null;
  }
  return null;
}

function detach(path: string): { parent: MockFolder; name: string; node: MockFolder } | null {
  const i = path.lastIndexOf(SEP);
  const parent = findFolder(path.slice(0, i));
  const name = path.slice(i + 1);
  const node = parent?.children[name];
  return parent && node ? { parent, name, node } : null;
}

/** 없는 폴더는 만들면서 찾는다(규칙의 연도·학기 폴더) */
function ensureFolder(path: string): MockFolder | null {
  for (const [rootPath, tree] of trees) {
    if (path === rootPath) return tree;
    if (!path.startsWith(rootPath + SEP)) continue;
    let node = tree;
    for (const part of path.slice(rootPath.length + 1).split(SEP)) {
      node.children[part] ??= folder(0);
      node = node.children[part];
    }
    return node;
  }
  return null;
}

function entry(path: string, name: string, node: MockFolder): FolderEntry {
  return {
    name,
    path: path + SEP + name,
    fileCount: node.files,
    folderCount: Object.keys(node.children).length,
  };
}

// ---------- 가짜 새 파일·정리함·기록 ----------
const file = (dir: string, name: string, size: number): PendingFile => ({
  path: dir + SEP + name,
  name,
  sourceDir: dir,
  size,
});

let pending: PendingFile[] =
  params.get("window") === "popup"
    ? [
        file(DOWNLOADS, "2027_1학기_3-2_수행평가_채점기준.hwp", 48_000),
        file(DOWNLOADS, "학업성적관리위원회_회의록_10월.hwp", 31_000),
        file(DESKTOP, "체험학습_단체사진_01.jpg", 2_400_000),
      ]
    : [];

// 주소 뒤 &many=1: 정리함에 파일이 아주 많은 경우(긴 목록 화면 확인용)
/** 정리함 줄(추천 위치는 목록을 줄 때 붙인다) */
type StoredItem = Omit<InboxItem, "suggestion">;

const manyFiles: StoredItem[] =
  params.get("many") === "1"
    ? Array.from({ length: 30 }, (_, i) => ({
        id: 1000 + i,
        ...file(
          [DOWNLOADS, DESKTOP, `${HOME}\\Documents`, "D:\\"][i % 4],
          `자료_${String(i + 1).padStart(2, "0")}.pdf`,
          50_000 + i * 1000,
        ),
        addedAt: Date.now() - i * 3600_000,
      }))
    : [];

let inbox: StoredItem[] = startReady
  ? [
      ...manyFiles,
      { id: 1, ...file(DOWNLOADS, "2학기_지필고사_감독배정표.xlsx", 22_000), addedAt: Date.now() - 12 * 60_000 },
      { id: 2, ...file(DESKTOP, "3-2_학부모상담_일정.xlsx", 15_000), addedAt: Date.now() - 3 * 3600_000 },
      { id: 3, ...file(DOWNLOADS, "scan_20261013.pdf", 820_000), addedAt: Date.now() - 26 * 3600_000 },
    ]
  : [];

interface MockMove {
  id: number;
  original: PendingFile;
  destDir: string;
  undone: boolean;
}

// 다운로드·바탕화면에 원래 있던 파일(‘지금 있는 파일 가져오기’로 불러옴)
let existingFiles: PendingFile[] = startReady
  ? [
      file(DOWNLOADS, "2027학년도_교육과정_편성_안내.pdf", 1_200_000),
      file(DESKTOP, "체험학습_단체사진_01.jpg", 2_400_000),
      file(DOWNLOADS, "광합성_수업자료.pptx", 5_100_000),
      file(DOWNLOADS, "학업성적관리위원회_회의록_10월.hwp", 31_000),
    ]
  : [];
const moves: MockMove[] = [];
let recent: string[] = startReady
  ? [`${readyRoot}\\업무\\회의록`, `${readyRoot}\\평가\\지필고사`, `${readyRoot}\\학급\\상담`]
  : [];

// ---------- 가짜 이벤트 ----------
type Handler = (payload: unknown) => void;
const handlers = new Map<string, Set<Handler>>();
export const mockEvents = {
  on(name: string, handler: Handler) {
    if (!handlers.has(name)) handlers.set(name, new Set());
    handlers.get(name)!.add(handler);
    return () => handlers.get(name)?.delete(handler);
  },
  emit(name: string, payload?: unknown) {
    setTimeout(() => handlers.get(name)?.forEach((h) => h(payload)), 0);
  },
};

const delay = <T>(value: T) => new Promise<T>((resolve) => setTimeout(() => resolve(value), 120));
const fail = (message: string) =>
  new Promise<never>((_, reject) => setTimeout(() => reject(message), 120));

type Trashed =
  | { kind: "file"; item: StoredItem }
  | { kind: "folder"; path: string; parent: MockFolder; name: string; node: MockFolder };
const trashed = new Map<number, Trashed>();

// ---------- 가짜 규칙(Rust rules.rs·naming.rs를 간단히 흉내) ----------
interface MockRule extends Rule {
  misses: number;
  missDir: string | null;
  deleted: boolean;
}

const mockRules: MockRule[] =
  startReady && params.get("rules") !== "0"
    ? [
        { words: ["수행평가"], extensions: [], sourceDir: null, dest: "평가\\수행평가\\2027\\1학기", split: true, used: 12 },
        { words: ["회의록"], extensions: [], sourceDir: null, dest: "업무\\회의록", split: false, used: 5 },
        { words: ["지필고사"], extensions: [], sourceDir: null, dest: "평가\\지필고사", split: false, used: 3 },
        { words: [], extensions: ["jpg", "png"], sourceDir: DESKTOP, dest: "학급\\사진", split: false, used: 8 },
      ].map((r, i) => ({
        id: i + 1,
        words: r.words,
        anyWord: false,
        extensions: r.extensions,
        sourceDir: r.sourceDir,
        destDir: `${readyRoot}\\${r.dest}`,
        split: r.split,
        used: r.used,
        misses: 0,
        missDir: null,
        deleted: false,
      }))
    : [];

const plainRule = ({ misses: _m, missDir: _d, deleted: _x, ...rule }: MockRule): Rule => ({ ...rule });
const liveRules = () => mockRules.filter((r) => !r.deleted);
const stemOf = (name: string) => (name.lastIndexOf(".") > 0 ? name.slice(0, name.lastIndexOf(".")) : name);
const extOf = (name: string) => (name.lastIndexOf(".") > 0 ? name.slice(name.lastIndexOf(".") + 1).toLowerCase() : "");

function templateOf(name: string): string | null {
  if (/^20\d\d$/.test(name)) return "[연도]";
  if (/^[12]학기$/.test(name)) return "[학기]";
  if (/^20\d\d [12]학기$/.test(name)) return "[연도] [학기]";
  if (/^[1-6]-\d{1,2}$/.test(name)) return "[학급]";
  return null;
}

function relNames(dest: string): string[] | null {
  const root = settings.rootFolder;
  if (!root || !dest.startsWith(root + SEP)) return null;
  return dest.slice(root.length + 1).split(SEP);
}

function describe(dest: string): DestSegment[] {
  const names = relNames(dest);
  if (!names) return [{ name: dest, template: null }];
  return names.map((name) => ({ name, template: templateOf(name) }));
}

function fill(template: string, fileName: string): string {
  const year = /(?<!\d)(20\d\d)(?!\d)/.exec(fileName)?.[1] ?? "2026";
  const term = /(?<!\d)([12])학기/.exec(fileName)?.[1] ?? "2";
  const cls = /(?<![\d-])([1-6])-(\d{1,2})(?![\d-])/.exec(fileName);
  return template
    .replace("[연도]", year)
    .replace("[학기]", `${term}학기`)
    .replace("[학급]", cls ? `${cls[1]}-${cls[2]}` : "")
    .trim();
}

function matchRule(fileName: string, sourceDir: string): MockRule | undefined {
  const stem = stemOf(fileName).toLowerCase();
  const ext = extOf(fileName);
  const fits = (r: MockRule) =>
    (r.extensions.length === 0 || r.extensions.includes(ext)) &&
    (!r.sourceDir || r.sourceDir === sourceDir) &&
    (r.anyWord ? r.words.length === 0 || r.words.some((w) => stem.includes(w.toLowerCase())) : r.words.every((w) => stem.includes(w.toLowerCase())));
  const live = liveRules();
  return live.filter((r) => r.words.length > 0).find(fits) ?? live.filter((r) => r.words.length === 0).find(fits);
}

function resolveRule(rule: Rule, fileName: string): { dir: string; exists: boolean; reveal: string } | null {
  const root = settings.rootFolder;
  const names = relNames(rule.destDir);
  if (!rule.split || !root || !names || !names.some(templateOf)) {
    return findFolder(rule.destDir) ? { dir: rule.destDir, exists: true, reveal: rule.destDir } : null;
  }
  const first = names.findIndex((n) => templateOf(n));
  let dir = [root, ...names.slice(0, first)].join(SEP);
  if (!findFolder(dir)) return null;
  for (const n of names.slice(first)) {
    const t = templateOf(n);
    const part = t ? fill(t, fileName) : n;
    if (part) dir += SEP + part;
  }
  let reveal = dir;
  while (!findFolder(reveal) && reveal.includes(SEP)) reveal = reveal.slice(0, reveal.lastIndexOf(SEP));
  return { dir, exists: !!findFolder(dir), reveal };
}

// 자동 분류 단어(Rust classify.rs의 기본 단어를 줄여 흉내)
const DEFAULT_WORDS: WordGroup[] = [
  { folder: ["업무", "공문"], words: ["공문", "시행문", "협조", "회신"] },
  { folder: ["업무", "회의록"], words: ["회의록", "협의회", "회의"] },
  { folder: ["업무", "계획서"], words: ["계획서", "계획"] },
  { folder: ["업무", "가정통신문"], words: ["가정통신문", "통신문"] },
  { folder: ["수업"], words: ["수업", "학습지", "활동지", "교안", "지도안"] },
  { folder: ["평가", "수행평가"], words: ["수행평가", "채점기준", "루브릭"] },
  { folder: ["평가", "지필고사"], words: ["지필", "중간고사", "기말고사", "문항정보표", "이원목적", "시험지", "감독"] },
  { folder: ["학급", "상담"], words: ["상담"] },
  { folder: ["학급", "출결"], words: ["출결", "결석", "지각", "조퇴"] },
  { folder: ["생활기록부"], words: ["생활기록부", "생기부", "세특", "세부능력", "행동특성", "창체", "진로활동", "동아리"] },
  { folder: ["연수"], words: ["연수", "이수증"] },
];
let autoWords: WordGroup[] = DEFAULT_WORDS.map((g) => ({ folder: [...g.folder], words: [...g.words] }));

const TYPES: [string, string[]][] = [
  ["한글 문서", ["hwp", "hwpx"]],
  ["워드 문서", ["doc", "docx"]],
  ["PDF", ["pdf"]],
  ["엑셀", ["xls", "xlsx", "csv"]],
  ["발표 자료", ["ppt", "pptx"]],
  ["사진", ["jpg", "jpeg", "png", "gif", "heic"]],
  ["동영상", ["mp4", "mov"]],
  ["압축 파일", ["zip", "7z", "egg"]],
];

/** 정리 폴더 아래 폴더(얕은 것부터, 형식별 폴더 안은 빼고) */
function allFolders(): { name: string; path: string }[] {
  const root = settings.rootFolder;
  const tree = root ? trees.get(root) : null;
  if (!root || !tree) return [];
  const out: { name: string; path: string }[] = [];
  const queue: [string, MockFolder, number][] = [[root, tree, 0]];
  while (queue.length) {
    const [path, node, depth] = queue.shift()!;
    for (const name of Object.keys(node.children).sort()) {
      if (depth === 0 && name === "형식별") continue;
      out.push({ name, path: path + SEP + name });
      queue.push([path + SEP + name, node.children[name], depth + 1]);
    }
  }
  return out;
}

function targetOf(dir: string) {
  let reveal = dir;
  while (!findFolder(reveal) && reveal.includes(SEP)) reveal = reveal.slice(0, reveal.lastIndexOf(SEP));
  return { destDir: dir, exists: !!findFolder(dir), reveal };
}

function suggestFor(fileName: string, sourceDir: string): Suggestion | null {
  const rule = matchRule(fileName, sourceDir);
  const target = rule ? resolveRule(rule, fileName) : null;
  if (rule && target) return { reason: { kind: "rule", rule: plainRule(rule) }, ...targetOf(target.dir) };
  const stem = stemOf(fileName).toLowerCase();
  const folders = allFolders().filter(
    (f) => f.name.length >= 2 && !/^\d+$/.test(f.name) && !templateOf(f.name),
  );
  let best: Suggestion | null = null;
  let bestLen = 0;
  for (const f of folders) {
    if (stem.includes(f.name.toLowerCase()) && f.name.length > bestLen) {
      best = { reason: { kind: "folder", word: f.name }, ...targetOf(f.path) };
      bestLen = f.name.length;
    }
  }
  for (const g of autoWords) {
    for (const w of g.words) {
      if (stem.includes(w.toLowerCase()) && w.length > bestLen) {
        const leaf = g.folder[g.folder.length - 1];
        const found = folders.find((f) => f.name === leaf);
        const dir = found ? found.path : [settings.rootFolder, ...g.folder].join(SEP);
        best = { reason: { kind: "word", word: w }, ...targetOf(dir) };
        bestLen = w.length;
      }
    }
  }
  return best;
}

function classifyFor(fileName: string, sourceDir: string): Suggestion | null {
  const sug = suggestFor(fileName, sourceDir);
  if (sug) return sug;
  const type = TYPES.find(([, exts]) => exts.includes(extOf(fileName)))?.[0];
  return type
    ? { reason: { kind: "type", typeName: type }, ...targetOf([settings.rootFolder, "형식별", type].join(SEP)) }
    : null;
}

function putPlanned(paths: string[], plan: (item: StoredItem) => Suggestion | null): PutOutcome[] {
  return paths.map((path) => {
    const item = inbox.find((i) => i.path === path);
    const sug = item ? plan(item) : null;
    if (!item || !sug) return { path, moveId: null, error: "이름이나 형식으로 넣을 곳을 정하지 못했어요." };
    const dest = ensureFolder(sug.destDir);
    if (dest) dest.files += 1;
    if (sug.reason.kind === "rule") {
      const id = sug.reason.rule.id;
      const r = mockRules.find((x) => x.id === id);
      if (r) r.used += 1;
    }
    const id = ++nextId;
    moves.push({ id, original: { ...item }, destDir: sug.destDir, undone: false });
    inbox = inbox.filter((i) => i.path !== path);
    return { path, moveId: id, error: null };
  });
}

function draftFor(fileName: string, destDir: string): RuleDraft {
  const words: string[] = [];
  for (const raw of stemOf(fileName).split(/[\s_.,()[\]]+/)) {
    for (const w of /^[1-6]-\d{1,2}$/.test(raw) ? [raw] : raw.split("-")) {
      if (w && !words.some((x) => x.toLowerCase() === w.toLowerCase())) words.push(w);
    }
  }
  const segments = describe(destDir);
  const exampleSegments = segments
    .map((s) => (s.template ? fill(s.template, "2028_2학기_1-1") : s.name))
    .filter(Boolean);
  const kinds = segments.map((s) => s.template ?? "").join("");
  return {
    words,
    preselected: words.filter((w) => !templateOf(w) && segments.some((s) => s.name === w)),
    destDir,
    segments,
    exampleLabel: [
      kinds.includes("[연도]") ? "2028년" : "",
      kinds.includes("[학기]") ? "2학기" : "",
      kinds.includes("[학급]") ? "1학년 1반" : "",
    ]
      .filter(Boolean)
      .join(" "),
    exampleSegments,
  };
}

function checkRule(input: RuleInput): RuleInput | string {
  const words = [...new Set(input.words.map((w) => w.trim()).filter(Boolean))];
  const extensions = [...new Set(input.extensions.map((e) => e.trim().replace(/^\.+/, "").toLowerCase()).filter(Boolean))];
  if (words.length === 0 && extensions.length === 0) return "조건을 하나 이상 정해 주세요(파일 이름의 단어나 확장자).";
  if (!input.destDir || !findFolder(input.destDir)) return "넣을 폴더를 찾을 수 없어요.";
  return { ...input, words, extensions };
}

let autoMoved: AutoMoved[] = [];

/** 가짜 "PC 전체 읽기": ?indexing=1이면 검색 화면을 연 뒤 몇 초 동안 읽는 중 */
const MOCK_INDEX_MS = 4000;
const MOCK_PC_FILES = 184_302;
let indexStartedAt: number | null = null;

function mockIndexState(): { wholePc: boolean; indexing: boolean; indexed: number } {
  if (!settings.searchWholePc) return { wholePc: false, indexing: false, indexed: 1_240 };
  if (params.get("indexing") === "1") {
    const elapsed = indexStartedAt === null ? 0 : Date.now() - indexStartedAt;
    if (elapsed < MOCK_INDEX_MS) {
      return { wholePc: false, indexing: true, indexed: Math.round((MOCK_PC_FILES * elapsed) / MOCK_INDEX_MS) };
    }
  }
  return { wholePc: true, indexing: false, indexed: MOCK_PC_FILES };
}

/** 정리 폴더 밖(PC 전체에서만 보이는) 가짜 파일 */
function mockPcFiles(): SearchHit[] {
  const day = 86_400_000;
  const rows: [string, string, number][] = [
    [`${HOME}\\Documents\\예전 자료\\2025`, "2025_2학기_수행평가_채점기준.hwp", 400],
    [`${HOME}\\Documents\\예전 자료\\2025`, "2025_학급운영계획.hwp", 420],
    [`${HOME}\\Downloads\\받은 파일`, "가정통신문_수학여행 안내.pdf", 12],
    [`${HOME}\\OneDrive\\학교`, "3-2_상담 기록.xlsx", 30],
    ["D:\\자료\\연수", "2026 교원 연수 이수증.pdf", 200],
    ["D:\\자료\\수업", "1학기 수업 계획서.hwpx", 210],
  ];
  return rows.map(([dir, name, ago]) => {
    const year = name.match(/20\d\d/)?.[0];
    const term = name.match(/([12])학기/)?.[1];
    const klass = name.match(/\d-\d/)?.[0];
    return {
      path: dir + SEP + name,
      name,
      dir,
      size: 52_000,
      modified: Date.now() - ago * day,
      year: year ? Number(year) : null,
      term: term ? Number(term) : null,
      class: klass ?? null,
      rule: null,
    };
  });
}

/** 검색용 가짜 파일: 가짜 폴더마다 파일 이름을 만들어 둔다 */
function mockSearchFiles(): SearchHit[] {
  const root = settings.rootFolder;
  const tree = root ? trees.get(root) : null;
  if (!root || !tree) return [];
  const samples = ["채점기준", "안내", "결과", "계획", "명단"];
  const out: SearchHit[] = [];
  const walk = (path: string, node: MockFolder, names: string[]) => {
    for (let i = 0; i < Math.min(node.files, 5); i++) {
      const year = names.find((n) => /^20\d\d$/.test(n)) ?? (i % 2 ? "2026" : "2027");
      const term = names.find((n) => /^[12]학기$/.test(n)) ?? `${(i % 2) + 1}학기`;
      const klass = i % 3 === 0 ? "_3-2" : i % 3 === 1 ? "_1-4" : "";
      const name = `${year}_${term}${klass}_${names[names.length - 1] ?? "자료"}_${samples[i % samples.length]}.${i % 2 ? "pdf" : "hwp"}`;
      out.push({
        path: path + SEP + name,
        name,
        dir: path,
        size: 30_000 + i * 1000,
        modified: Date.now() - (out.length + 1) * 86_400_000,
        year: Number(year),
        term: Number(term[0]),
        class: klass ? klass.slice(1) : null,
        rule: names.includes("수행평가") ? "수행평가" : null,
      });
    }
    for (const [n, child] of Object.entries(node.children)) walk(path + SEP + n, child, [...names, n]);
  };
  walk(root, tree, []);
  return out;
}
let paused = false;

let pickCount = 0;
let nextId = 100;

export const mockBackend = {
  appInfo: () => delay<AppInfo>({ name: "파일 정리 도우미", version: "0.1.0", os: "browser" }),
  setupInfo: () => delay(setupInfo),
  getSettings: () => delay({ ...settings }),

  completeSetup(req: SetupRequest) {
    if (req.watchFolders.length === 0) return fail("지켜볼 폴더를 하나 이상 골라 주세요.");
    const root =
      req.preset === "existing" ? req.baseDir : `${req.baseDir}${SEP}${setupInfo.rootFolderName}`;
    if (req.watchFolders.includes(root)) return fail("정리 폴더와 지켜볼 폴더는 서로 달라야 해요.");
    if (!trees.has(root)) {
      const build = (specs: { name: string; children: unknown[] }[]): Record<string, MockFolder> =>
        Object.fromEntries(
          specs.map((s) => [s.name.trim(), folder(0, build(s.children as { name: string; children: unknown[] }[]))]),
        );
      trees.set(
        root,
        req.preset === "empty"
          ? folder(0)
          : req.structure
            ? folder(0, build(req.structure))
            : teacherTree(),
      );
    }
    settings = {
      ...settings,
      setupDone: true,
      rootFolder: root,
      watchFolders: req.watchFolders,
      organizeMode: req.organizeMode,
    };
    return delay({ ...settings });
  },

  setOrganizeMode(mode: OrganizeMode) {
    settings = { ...settings, organizeMode: mode };
    return delay({ ...settings });
  },

  setWatchFolders(folders: string[]) {
    if (folders.length === 0) return fail("지켜볼 폴더를 하나 이상 골라 주세요.");
    settings = { ...settings, watchFolders: folders };
    return delay({ ...settings });
  },

  listFolders(path: string) {
    const node = findFolder(path);
    if (!node) return fail("폴더를 읽지 못했어요.");
    const entries = Object.entries(node.children)
      .map(([name, child]) => entry(path, name, child))
      .sort((a, b) => a.name.localeCompare(b.name, "ko"));
    return delay(entries);
  },

  createFolder(parent: string, name: string) {
    const node = findFolder(parent);
    const trimmed = name.trim();
    if (!node) return fail("폴더를 만들 위치를 찾을 수 없어요.");
    if (!trimmed) return fail("이름을 입력해 주세요.");
    if (/[\\/:*?"<>|]/.test(trimmed)) return fail("이름에 쓸 수 없는 글자가 있어요.");
    if (node.children[trimmed]) return fail(`‘${trimmed}’ 폴더가 이미 있어요.`);
    node.children[trimmed] = folder(0);
    return delay(entry(parent, trimmed, node.children[trimmed]));
  },

  searchFolders(query: string) {
    const q = query.trim().toLowerCase();
    const root = settings.rootFolder;
    const tree = root ? trees.get(root) : null;
    if (!q || !root || !tree) return delay<FolderMatch[]>([]);
    const out: FolderMatch[] = [];
    const queue: [string, string[], MockFolder][] = [[root, [], tree]];
    while (queue.length) {
      const [path, trail, node] = queue.shift()!;
      for (const [name, child] of Object.entries(node.children)) {
        const childTrail = [...trail, name];
        if (name.toLowerCase().includes(q)) out.push({ name, path: path + SEP + name, trail: childTrail });
        queue.push([path + SEP + name, childTrail, child]);
      }
    }
    return delay(out.slice(0, 50));
  },

  recentDestinations: () => delay(recent.slice(0, 3)),

  popupState() {
    const current = pending[0] ?? null;
    return delay<PopupState>({
      current,
      remaining: Math.max(0, pending.length - 1),
      suggestion: current ? suggestFor(current.name, current.sourceDir) : null,
      autoMoved: [...autoMoved],
    });
  },

  dismissAuto(ids: number[]) {
    autoMoved = autoMoved.filter((a) => !ids.includes(a.moveId));
    return delay(undefined);
  },

  putFile(path: string, destDir: string, newName: string | null) {
    const item = pending.find((p) => p.path === path) ?? inbox.find((i) => i.path === path);
    if (!item) return fail("파일을 찾을 수 없어요. 이미 옮겨졌거나 지워졌을 수 있어요.");
    const sug = suggestFor(item.name, item.sourceDir);
    const followed = sug && sug.destDir === destDir ? sug.reason : null;
    const reason = sug?.reason;
    const rule = reason?.kind === "rule" ? mockRules.find((r) => r.id === reason.rule.id) : undefined;
    let change: PutResult["change"] = null;
    if (sug && !followed && rule) {
      rule.misses = rule.missDir === destDir ? rule.misses + 1 : 1;
      rule.missDir = destDir;
      if (rule.misses >= 3) change = { rule: plainRule(rule), oldDir: sug.destDir, newDir: destDir };
    }
    const dest = followed ? ensureFolder(destDir) : findFolder(destDir);
    if (dest) dest.files += 1;
    if (followed && rule) {
      rule.used += 1;
      rule.misses = 0;
    }
    const id = ++nextId;
    moves.push({ id, original: { ...item }, destDir, undone: false });
    pending = pending.filter((p) => p.path !== path);
    inbox = inbox.filter((i) => i.path !== path);
    recent = [destDir, ...recent.filter((r) => r !== destDir)];
    mockEvents.emit("inbox-changed");
    return delay<PutResult>({
      moveId: id,
      movedPath: destDir + SEP + (newName ?? item.name),
      destDir,
      followed,
      change,
      draft: followed || change ? null : draftFor(item.name, destDir),
    });
  },

  putSuggested(paths: string[]) {
    const outcomes = putPlanned(paths, (i) => suggestFor(i.name, i.sourceDir));
    mockEvents.emit("inbox-changed");
    return delay(outcomes);
  },

  classifyPreview(paths: string[]) {
    return delay<ClassifyPlan[]>(
      paths.map((path) => {
        const item = inbox.find((i) => i.path === path);
        return { path, name: item?.name ?? path, target: item ? classifyFor(item.name, item.sourceDir) : null };
      }),
    );
  },

  classifyFiles(paths: string[]) {
    const outcomes = putPlanned(paths, (i) => classifyFor(i.name, i.sourceDir));
    mockEvents.emit("inbox-changed");
    return delay(outcomes);
  },

  getAutoWords: () => delay(autoWords.map((g) => ({ folder: [...g.folder], words: [...g.words] }))),

  setAutoWords(groups: WordGroup[] | null) {
    const next = (groups ?? DEFAULT_WORDS).map((g) => ({
      folder: g.folder.map((n) => n.trim()).filter(Boolean),
      words: [...new Set(g.words.map((w) => w.trim()).filter(Boolean))],
    }));
    if (next.some((g) => g.folder.length === 0)) return fail("넣을 폴더 이름을 적어 주세요.");
    const empty = next.find((g) => g.words.length === 0);
    if (empty) return fail(`‘${empty.folder.join(" › ")}’ 폴더에 단어를 하나 이상 적어 주세요.`);
    autoWords = next;
    mockEvents.emit("inbox-changed");
    return delay(next.map((g) => ({ folder: [...g.folder], words: [...g.words] })));
  },

  undoMove(moveId: number, requeue: boolean) {
    const move = moves.find((m) => m.id === moveId);
    if (!move) return fail("되돌릴 기록을 찾을 수 없어요.");
    if (move.undone) return fail("이미 되돌렸어요.");
    move.undone = true;
    autoMoved = autoMoved.filter((a) => a.moveId !== moveId);
    const dest = findFolder(move.destDir);
    if (dest) dest.files -= 1;
    if (requeue) pending = [move.original, ...pending];
    return delay(move.original);
  },

  deferFile(path: string) {
    const item = pending.find((p) => p.path === path);
    pending = pending.filter((p) => p.path !== path);
    if (item) inbox = [{ id: ++nextId, ...item, addedAt: Date.now() }, ...inbox];
    mockEvents.emit("inbox-changed");
    return delay(undefined);
  },

  hidePopup() {
    // 브라우저에서는 창을 숨길 수 없으니 새 파일 하나를 다시 넣어 계속 볼 수 있게 한다.
    setTimeout(() => {
      pending = [file(DOWNLOADS, `가정통신문_${++pickCount}.pdf`, 120_000)];
      mockEvents.emit("popup-changed");
    }, 1500);
    return delay(undefined);
  },

  putFiles(paths: string[], destDir: string) {
    const dest = findFolder(destDir);
    const outcomes: PutOutcome[] = paths.map((path) => {
      const item = inbox.find((i) => i.path === path) ?? pending.find((p) => p.path === path);
      if (!item || !dest) {
        return { path, moveId: null, error: "파일을 찾을 수 없어요. 이미 옮겨졌거나 지워졌을 수 있어요." };
      }
      dest.files += 1;
      const id = ++nextId;
      moves.push({ id, original: { ...item }, destDir, undone: false });
      inbox = inbox.filter((i) => i.path !== path);
      pending = pending.filter((p) => p.path !== path);
      return { path, moveId: id, error: null };
    });
    if (outcomes.some((o) => o.moveId)) recent = [destDir, ...recent.filter((r) => r !== destDir)];
    mockEvents.emit("inbox-changed");
    return delay(outcomes);
  },

  undoMoves(moveIds: number[]) {
    const result: UndoManyResult = { restored: 0, errors: [] };
    for (const id of moveIds) {
      const move = moves.find((m) => m.id === id);
      if (!move || move.undone) {
        result.errors.push("되돌릴 기록을 찾을 수 없어요.");
        continue;
      }
      move.undone = true;
      const dest = findFolder(move.destDir);
      if (dest) dest.files -= 1;
      inbox = [{ id: ++nextId, ...move.original, addedAt: Date.now() }, ...inbox];
      result.restored += 1;
    }
    mockEvents.emit("inbox-changed");
    return delay(result);
  },

  importExisting() {
    const added = existingFiles.filter((f) => !inbox.some((i) => i.path === f.path));
    inbox = [...inbox, ...added.map((f) => ({ id: ++nextId, ...f, addedAt: Date.now() - 3 * 86400_000 }))];
    existingFiles = [];
    mockEvents.emit("inbox-changed");
    return delay(added.length);
  },

  favoriteFolders() {
    const root = settings.rootFolder;
    const tree = root ? trees.get(root) : null;
    const counts = new Map<string, number>();
    for (const m of moves.filter((m) => !m.undone)) counts.set(m.destDir, (counts.get(m.destDir) ?? 0) + 1);
    const frequent = [...counts.entries()].sort((a, b) => b[1] - a[1]).map(([d]) => d);
    const seed = startReady ? [`${readyRoot}\\업무\\공문`, `${readyRoot}\\평가\\수행평가`, `${readyRoot}\\학급\\상담`] : [];
    const auto = [...new Set([...frequent, ...seed])].filter((p) => findFolder(p));
    if (root && tree) {
      for (const name of Object.keys(tree.children).sort((a, b) => a.localeCompare(b, "ko"))) {
        const p = root + SEP + name;
        if (!auto.includes(p)) auto.push(p);
      }
    }
    // Rust settings::merge_favorites 흉내: 직접 고른 칸은 그 폴더, 나머지는 자동
    const pins = settings.pinnedFavorites;
    const rest = auto.filter((a) => !pins.includes(a));
    const out: string[] = [];
    for (let i = 0; i < 6; i++) {
      const pin = pins[i];
      const pick = pin && findFolder(pin) && !out.includes(pin) ? pin : rest.find((a) => !out.includes(a));
      if (pick) out.push(pick);
    }
    return delay(out);
  },

  renameFolder(path: string, newName: string) {
    const d = detach(path);
    const name = newName.trim();
    if (!d) return fail("폴더를 찾을 수 없어요.");
    if (!name) return fail("이름을 입력해 주세요.");
    if (d.parent.children[name] && name !== d.name) return fail(`‘${name}’ 폴더가 이미 있어요.`);
    delete d.parent.children[d.name];
    d.parent.children[name] = d.node;
    mockEvents.emit("inbox-changed");
    return delay(entry(path.slice(0, path.lastIndexOf(SEP)), name, d.node));
  },

  moveFolder(path: string, destParent: string) {
    const d = detach(path);
    const dest = findFolder(destParent);
    if (!d || !dest) return fail("폴더를 찾을 수 없어요.");
    if (destParent === path || destParent.startsWith(path + SEP)) return fail("폴더를 자기 안으로 옮길 수는 없어요.");
    if (dest === d.parent) return fail("이미 그 폴더 안에 있어요.");
    if (dest.children[d.name]) return fail(`옮길 곳에 ‘${d.name}’ 폴더가 이미 있어요.`);
    delete d.parent.children[d.name];
    dest.children[d.name] = d.node;
    mockEvents.emit("inbox-changed");
    return delay(entry(destParent, d.name, d.node));
  },

  trashFolder(path: string) {
    const d = detach(path);
    if (!d) return fail("지울 폴더를 찾을 수 없어요.");
    delete d.parent.children[d.name];
    const id = ++nextId;
    trashed.set(id, { kind: "folder", path, parent: d.parent, name: d.name, node: d.node });
    mockEvents.emit("inbox-changed");
    return delay(id);
  },

  trashFiles(paths: string[]) {
    const result: TrashResult = { deletionIds: [], errors: [] };
    for (const path of paths) {
      const item = inbox.find((i) => i.path === path);
      if (!item) {
        result.errors.push("파일을 찾을 수 없어요.");
        continue;
      }
      inbox = inbox.filter((i) => i.path !== path);
      const id = ++nextId;
      trashed.set(id, { kind: "file", item });
      result.deletionIds.push(id);
    }
    mockEvents.emit("inbox-changed");
    return delay(result);
  },

  restoreDeleted(ids: number[]) {
    const result: UndoManyResult = { restored: 0, errors: [] };
    for (const id of ids) {
      const t = trashed.get(id);
      if (!t) {
        result.errors.push("되돌릴 기록을 찾을 수 없어요.");
        continue;
      }
      if (t.kind === "file") inbox = [t.item, ...inbox];
      else t.parent.children[t.name] = t.node;
      trashed.delete(id);
      result.restored += 1;
    }
    mockEvents.emit("inbox-changed");
    return delay(result);
  },

  // ---------- 파일 검색(가짜 폴더의 파일 이름을 만들어 찾는다) ----------
  // ?indexing=1 이면 처음 몇 초 동안 "PC 전체를 읽는 중"(그동안은 정리 폴더에서만)을 흉내 낸다.
  searchFiles(query: string) {
    const everything: EverythingState = params.get("everything") === "1" ? "ready" : "notRunning";
    const index = mockIndexState();
    const source: SearchSource = everything === "ready" ? "everything" : index.wholePc ? "pc" : "folders";
    const base = { source, everything, wantWholePc: settings.searchWholePc, indexing: index.indexing, indexed: index.indexed };
    const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (terms.length === 0) return delay<SearchResponse>({ ...base, hits: [] });
    const all = source === "folders" ? mockSearchFiles() : [...mockSearchFiles(), ...mockPcFiles()];
    const hits = all
      .filter((h) => terms.every((t) => h.path.toLowerCase().includes(t)))
      .sort((a, b) => b.modified - a.modified)
      .slice(0, 200);
    return delay<SearchResponse>({ ...base, hits });
  },

  prepareSearch() {
    if (params.get("indexing") === "1" && indexStartedAt === null) {
      indexStartedAt = Date.now();
      window.setTimeout(() => mockEvents.emit("search-indexed"), MOCK_INDEX_MS + 50);
    }
    return delay(undefined);
  },

  setSearchWholePc(enabled: boolean) {
    settings = { ...settings, searchWholePc: enabled };
    return delay({ ...settings });
  },

  openPath: (path: string) => (path.includes("없는") ? fail("파일을 찾을 수 없어요.") : delay(undefined)),
  revealPath: (_path: string) => delay(undefined),

  searchInfo: () => {
    const index = mockIndexState();
    return delay<SearchInfo>({
      everything: params.get("everything") === "1" ? "ready" : "notRunning",
      wholePc: index.wholePc,
      indexing: index.indexing,
      indexed: index.indexed,
      shortcutError: null,
    });
  },

  setSearchShortcut(shortcut: string) {
    settings = { ...settings, searchShortcut: shortcut };
    return delay({ ...settings });
  },

  getStatus: () => delay({ paused }),

  setPaused(next: boolean) {
    paused = next;
    mockEvents.emit("status-changed");
    return delay({ paused });
  },

  setAutostart(enabled: boolean) {
    settings = { ...settings, autostart: enabled };
    return delay({ ...settings });
  },

  setWaitSeconds(seconds: number) {
    settings = { ...settings, waitSeconds: seconds };
    return delay({ ...settings });
  },

  setPinnedFavorites(pins: (string | null)[]) {
    settings = { ...settings, pinnedFavorites: pins };
    mockEvents.emit("inbox-changed");
    return delay({ ...settings });
  },

  setRootFolder(path: string) {
    if (settings.watchFolders.includes(path)) return fail("정리 폴더와 지켜볼 폴더는 서로 달라야 해요.");
    if (!trees.has(path)) trees.set(path, folder(0, { "새로 고른 폴더 안": folder(2) }));
    settings = { ...settings, rootFolder: path, pinnedFavorites: [] };
    mockEvents.emit("inbox-changed");
    return delay({ ...settings });
  },

  listInbox: () => delay(inbox.map((i) => ({ ...i, suggestion: suggestFor(i.name, i.sourceDir) }))),

  setPlaceMode(mode: PlaceMode) {
    settings = { ...settings, placeMode: mode };
    return delay({ ...settings });
  },

  setTermMonths(first: number, second: number) {
    if (!(first >= 1 && second <= 12 && first < second)) return fail("1학기가 2학기보다 먼저 시작해야 해요.");
    settings = { ...settings, firstTermMonth: first, secondTermMonth: second };
    mockEvents.emit("inbox-changed");
    return delay({ ...settings });
  },

  listRules: () =>
    delay<RuleView[]>(
      liveRules().map((r) => ({
        ...plainRule(r),
        segments: describe(r.destDir),
        missing: resolveRule(r, "") === null,
      })),
    ),

  describeDest: (destDir: string) => delay(describe(destDir)),

  createRule(input: RuleInput) {
    const checked = checkRule(input);
    if (typeof checked === "string") return fail(checked);
    const rule: MockRule = { id: ++nextId, ...checked, used: 0, misses: 0, missDir: null, deleted: false };
    mockRules.unshift(rule);
    mockEvents.emit("inbox-changed");
    return delay(plainRule(rule));
  },

  updateRule(id: number, input: RuleInput) {
    const checked = checkRule(input);
    if (typeof checked === "string") return fail(checked);
    const rule = mockRules.find((r) => r.id === id);
    if (!rule) return fail("규칙을 찾을 수 없어요.");
    Object.assign(rule, checked, { misses: 0, missDir: null });
    mockEvents.emit("inbox-changed");
    return delay(plainRule(rule));
  },

  deleteRule(id: number) {
    const rule = mockRules.find((r) => r.id === id);
    if (rule) rule.deleted = true;
    mockEvents.emit("inbox-changed");
    return delay(undefined);
  },

  restoreRule(id: number) {
    const rule = mockRules.find((r) => r.id === id);
    if (rule) rule.deleted = false;
    mockEvents.emit("inbox-changed");
    return delay(undefined);
  },

  moveRule(id: number, up: boolean) {
    const live = liveRules();
    const i = live.findIndex((r) => r.id === id);
    const j = up ? i - 1 : i + 1;
    if (i < 0 || j < 0 || j >= live.length) return delay(false);
    const a = mockRules.indexOf(live[i]);
    const b = mockRules.indexOf(live[j]);
    [mockRules[a], mockRules[b]] = [mockRules[b], mockRules[a]];
    mockEvents.emit("inbox-changed");
    return delay(true);
  },

  answerRuleChange(id: number, destDir: string | null) {
    const rule = mockRules.find((r) => r.id === id);
    if (!rule) return fail("규칙을 찾을 수 없어요.");
    if (destDir) rule.destDir = destDir;
    rule.misses = 0;
    rule.missDir = null;
    mockEvents.emit("inbox-changed");
    return delay(undefined);
  },

  pickFolder() {
    pickCount += 1;
    return delay(`${HOME}\\Documents\\고른 폴더 ${pickCount}`);
  },

  // ---------- 이름 한꺼번에 바꾸기(Rust renamer.rs를 줄여 옮긴 것: 화면 확인용) ----------
  renameList(folder: string, includeSub: boolean) {
    if (/^[A-Z]:\\?$/.test(folder)) return fail("드라이브 맨 위는 고를 수 없어요. 그 안의 폴더를 골라 주세요.");
    const files = mockRenameFolder(folder).filter((f) => includeSub || f.subDir === "");
    return delay<FolderFiles>({ folder, files: files.map((f) => ({ ...f })), truncated: false });
  },

  renamePreview(request: RenameRequest) {
    return delay(mockPlan(request));
  },

  renameApply(request: RenameRequest) {
    const plan = mockPlan(request);
    const batch = ++renameBatch;
    const done: { file: RenameFile; from: string }[] = [];
    for (const row of plan.rows) {
      if (row.status !== "rename" && row.status !== "conflict") continue;
      const file = [...renameFolders.values()].flat().find((f) => f.path === row.path);
      if (!file) continue;
      done.push({ file, from: file.name });
      file.name = row.newName;
      file.path = file.path.slice(0, file.path.length - row.oldName.length) + row.newName;
    }
    renameBatches.set(batch, done);
    return delay<RenameResult>({ batch, renamed: done.length, errors: [] });
  },

  renameUndo(batch: number) {
    const done = renameBatches.get(batch) ?? [];
    for (const { file, from } of done) {
      file.path = file.path.slice(0, file.path.length - file.name.length) + from;
      file.name = from;
    }
    renameBatches.delete(batch);
    return delay<UndoManyResult>({ restored: done.length, errors: [] });
  },
};

// ---------- 이름 바꾸기 가짜 폴더·이름 짓기 ----------
const renameFolders = new Map<string, RenameFile[]>();
const renameBatches = new Map<number, { file: RenameFile; from: string }[]>();
let renameBatch = 0;

function mockRenameFolder(folder: string): RenameFile[] {
  let files = renameFolders.get(folder);
  if (!files) {
    const now = Date.now();
    const make = (name: string, subDir: string, i: number): RenameFile => ({
      path: [folder, subDir, name].filter(Boolean).join(SEP),
      name,
      subDir,
      modified: now - i * 3_600_000,
      size: 24_000,
    });
    files = [
      ...Array.from({ length: 30 }, (_, i) => make(`${String(i + 1).padStart(2, "0")}.hwp`, "", 40 - i)),
      make("제출 안내.pdf", "", 50),
      make("31.hwp", "늦게 낸 것", 2),
      make("32번 과제.hwp", "늦게 낸 것", 1),
    ];
    renameFolders.set(folder, files);
  }
  return files;
}

type MockToken = { text: string; kind: PieceKind; value: number | null };

function mockTokens(stem: string): MockToken[] {
  const out: MockToken[] = [];
  for (const raw of stem.split(/[\s_()[\].,·{}]+/).filter(Boolean)) {
    const parts = /^[1-6]-\d{1,2}$/.test(raw) ? [raw] : raw.split("-").filter(Boolean);
    for (const t of parts) {
      let kind: PieceKind = "text";
      let value: number | null = null;
      if (/^[1-6]-\d{1,2}$/.test(t)) kind = "class";
      else if (/^\d+$/.test(t)) {
        const n = Number(t);
        if (t.length === 4 && n >= 2000 && n <= 2099) kind = "year";
        else [kind, value] = ["number", t.length === 5 ? n % 100 : n];
      } else if (/^\d+번$/.test(t)) [kind, value] = ["number", Number(t.slice(0, -1))];
      else if (t === "1학기" || t === "2학기") kind = "term";
      else if (/^(\d{2}|\d{4})(학년도|년)$/.test(t)) kind = "year";
      out.push({ text: t, kind, value });
    }
  }
  return out;
}

function mockAnalyze(stems: string[]): NameAnalysis {
  const all = stems.map(mockTokens);
  const shapes = new Map<string, number>();
  for (const t of all) {
    const k = t.map((x) => x.kind).join(",");
    shapes.set(k, (shapes.get(k) ?? 0) + 1);
  }
  let base = "";
  let matching = 0;
  for (const [k, n] of shapes) if (n > matching) [base, matching] = [k, n];
  const group = all.filter((t) => t.map((x) => x.kind).join(",") === base);
  if (!base || group.length === 0) return { pieces: [], matching: 0, total: stems.length, numberPiece: null, suggestions: [] };
  const pieces: PieceInfo[] = group[0].map((tok, i) => ({
    index: i + 1,
    kind: tok.kind,
    common: group.every((t) => t[i].text.toLowerCase() === tok.text.toLowerCase()),
    sample: tok.text,
  }));
  const numberPiece = [...pieces].reverse().find((p) => p.kind === "number" && (!p.common || group.length === 1))?.index ?? null;
  const suggestions: NameAnalysis["suggestions"] = [];
  if (numberPiece !== null) {
    suggestions.push({ label: "번호로 된 이름이네요 → 학생 이름(번호)", template: "{학생 이름}({번호})", needsRoster: true });
    const c = pieces.find((p) => p.kind === "text" && p.common && p.index !== numberPiece);
    if (c) {
      suggestions.push({
        label: `‘${pieces.map((p) => p.sample).join("_")}’ 모양 → 번호_학생 이름_${c.sample}`,
        template: `{번호}_{학생 이름}_{조각 ${c.index}}`,
        needsRoster: true,
      });
      suggestions.push({ label: `${c.sample}(번호)`, template: `{조각 ${c.index}}({번호})`, needsRoster: false });
    }
  }
  return { pieces, matching, total: stems.length, numberPiece, suggestions };
}

function mockRoster(text: string): { names: Map<number, string>; info: RosterInfo } {
  const names = new Map<number, string>();
  const info: RosterInfo = { count: 0, ignored: 0, duplicates: [] };
  const header = ["번호", "이름", "성명", "학번", "성별", "번", "학생"];
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line) continue;
    const fields = (line.includes("\t") ? line.split("\t") : line.includes(",") ? line.split(",") : line.split(/\s+/))
      .map((f) => f.trim())
      .filter(Boolean);
    const numField = fields.find((f) => /^\d{1,6}번?$/.test(f));
    const digits = numField?.replace("번", "");
    const num = digits ? (digits.length === 5 ? Number(digits) % 100 : Number(digits)) : null;
    const name = fields.find((f) => !/\d/.test(f) && !header.includes(f));
    if (num && name) {
      if (names.has(num) && !info.duplicates.includes(num)) info.duplicates.push(num);
      names.set(num, name);
    } else if (!(num === null && (!name || fields.every((f) => header.includes(f))))) info.ignored += 1;
  }
  info.count = names.size;
  return { names, info };
}

const tidyName = (s: string) =>
  s
    .replace(/\s+/g, " ")
    .replace(/ {2,}/g, " ")
    .replace(/_{2,}/g, "_")
    .replace(/^[\s_-]+|[\s_-]+$/g, "");

function mockPlan(req: RenameRequest): RenamePreview {
  const names = req.files.map((p) => p.slice(p.lastIndexOf(SEP) + 1));
  const analysis = mockAnalyze(names.map(stemOf));
  const base = analysis.pieces.map((p) => p.kind).join(",");
  const roster = mockRoster(req.roster);
  const template = req.template.trim();
  const last = req.number.start + req.files.length - 1;
  const width = req.number.digits || (req.number.fromName ? 1 : String(last).length);
  const usesNumber = template.includes("{번호}") || template.includes("{학생 이름}");
  const rows: PreviewRow[] = req.files.map((path, i) => {
    const name = names[i];
    const stem = stemOf(name);
    const ext = name.slice(stem.length);
    const toks = mockTokens(stem);
    const matches = toks.map((t) => t.kind).join(",") === base;
    let num: number | null = null;
    if (req.number.fromName) {
      const fromPiece = matches && analysis.numberPiece ? toks[analysis.numberPiece - 1]?.value : null;
      num = fromPiece ?? [...toks].reverse().find((t) => t.kind === "number")?.value ?? null;
      if (num === null) {
        const m = stem.match(/(\d+)(?!.*\d)/);
        if (m && !(m[1].length === 4 && Number(m[1]) >= 2000)) num = Number(m[1]);
      }
    } else num = req.number.start + i;
    const numText = num === null ? null : String(num).padStart(width, "0");
    const row: PreviewRow = { path, oldName: name, newName: name, status: "same", note: null };
    let s = stem;
    if (template) {
      if (usesNumber && numText === null) return { ...row, status: "skip", note: "이름에서 번호를 찾지 못했어요" };
      let error: string | null = null;
      s = template.replace(/\{([^}]*)\}/g, (whole, slotRaw: string) => {
        const slot = slotRaw.trim();
        if (slot === "원래 이름") return stem;
        if (slot === "번호") return numText ?? "";
        if (slot === "학생 이름") {
          const st = num !== null ? roster.names.get(num) : undefined;
          if (!st) error = `명렬표에 ${num}번이 없어요`;
          return st ?? "";
        }
        if (slot === "연도") return stem.match(/20\d\d/)?.[0] ?? "";
        if (slot === "학기") return stem.match(/[12]학기/)?.[0] ?? "";
        if (slot === "학급") return stem.match(/[1-6]-\d{1,2}/)?.[0] ?? "";
        if (slot === "날짜") return new Date(Date.now()).toISOString().slice(0, 10);
        const n = slot.match(/^조각\s*(\d+)$/);
        if (n) {
          if (!matches || !toks[Number(n[1]) - 1]) error = "이름 모양이 다른 파일이에요";
          return toks[Number(n[1]) - 1]?.text ?? "";
        }
        return whole;
      });
      if (error) return { ...row, status: "skip", note: error };
    }
    for (const r of req.replacements) {
      if (!r.find) continue;
      s = s.replace(new RegExp(r.find.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "gi"), r.with);
    }
    s = tidyName(req.prefix + s + req.suffix);
    const newName = s + ext;
    if (!s) return { ...row, status: "invalid", note: "이름이 비어 있어요" };
    const bad = newName.match(/[\\/:*?"<>|]/);
    if (bad) return { ...row, status: "invalid", note: `이름에 ‘${bad[0]}’ 글자는 쓸 수 없어요.` };
    if (newName === name) return row;
    return { ...row, newName, status: "rename" };
  });
  // 겹침: 바꾸지 않는 파일 이름과 앞서 정한 새 이름을 피해 (2) (3) …
  const moving = new Set(rows.filter((r) => r.status === "rename").map((r) => r.path));
  const taken = new Map<string, Set<string>>();
  for (const r of rows) {
    if (r.status !== "rename") continue;
    const dir = r.path.slice(0, r.path.lastIndexOf(SEP));
    if (!taken.has(dir)) {
      const others = [...renameFolders.values()].flat().filter((f) => f.path.startsWith(dir + SEP) && !f.path.slice(dir.length + 1).includes(SEP));
      taken.set(dir, new Set(others.filter((f) => !moving.has(f.path)).map((f) => f.name.toLowerCase())));
    }
    const set = taken.get(dir)!;
    const stem = stemOf(r.newName);
    const ext = r.newName.slice(stem.length);
    let candidate = r.newName;
    let n = 1;
    while (set.has(candidate.toLowerCase())) candidate = `${stem} (${++n})${ext}`;
    set.add(candidate.toLowerCase());
    if (n > 1) Object.assign(r, { status: "conflict", note: `같은 이름이 있어 (${n})을 붙여요`, newName: candidate });
  }
  return { analysis, roster: roster.info, rows, changes: rows.filter((r) => r.status === "rename" || r.status === "conflict").length };
}

// 주소 뒤 &auto=1: "바로 정리"로 이미 옮긴 파일 알림부터 보이게
if (params.get("auto") === "1" && mockRules[0]) {
  const original = file(DOWNLOADS, "2027_1학기_3-1_수행평가_결과.hwp", 52_000);
  const id = ++nextId;
  moves.push({ id, original, destDir: `${readyRoot}\\평가\\수행평가\\2027\\1학기`, undone: false });
  autoMoved = [
    { moveId: id, name: original.name, destDir: moves[moves.length - 1].destDir, reason: { kind: "rule", rule: plainRule(mockRules[0]) } },
  ];
}
