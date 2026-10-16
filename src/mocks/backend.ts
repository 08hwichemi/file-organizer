// 브라우저 미리보기용 가짜 백엔드. 실제 파일은 건드리지 않고 메모리에서만 흉내 낸다.
// - 주소 뒤 ?mock=ready  : 처음 설정을 건너뛰고 정리함 화면부터
// - 주소 뒤 ?window=popup: 새 파일 팝업 창(새 파일 3개가 기다리는 상태)
import type {
  AppInfo,
  FolderEntry,
  FolderMatch,
  InboxItem,
  OrganizeMode,
  PendingFile,
  PopupState,
  PutOutcome,
  PutResult,
  Settings,
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
      watchFolders: [DOWNLOADS, DESKTOP],
      organizeMode: "ask",
    }
  : { setupDone: false, rootFolder: null, watchFolders: [], organizeMode: "ask" };

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
const manyFiles: InboxItem[] =
  params.get("many") === "1"
    ? Array.from({ length: 30 }, (_, i) => ({
        id: 1000 + i,
        ...file(i % 3 ? DOWNLOADS : DESKTOP, `자료_${String(i + 1).padStart(2, "0")}.pdf`, 50_000 + i * 1000),
        addedAt: Date.now() - i * 3600_000,
      }))
    : [];

let inbox: InboxItem[] = startReady
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
type Handler = () => void;
const handlers = new Map<string, Set<Handler>>();
export const mockEvents = {
  on(name: string, handler: Handler) {
    if (!handlers.has(name)) handlers.set(name, new Set());
    handlers.get(name)!.add(handler);
    return () => handlers.get(name)?.delete(handler);
  },
  emit(name: string) {
    setTimeout(() => handlers.get(name)?.forEach((h) => h()), 0);
  },
};

const delay = <T>(value: T) => new Promise<T>((resolve) => setTimeout(() => resolve(value), 120));
const fail = (message: string) =>
  new Promise<never>((_, reject) => setTimeout(() => reject(message), 120));

type Trashed =
  | { kind: "file"; item: InboxItem }
  | { kind: "folder"; path: string; parent: MockFolder; name: string; node: MockFolder };
const trashed = new Map<number, Trashed>();

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

  popupState: () =>
    delay<PopupState>({ current: pending[0] ?? null, remaining: Math.max(0, pending.length - 1) }),

  putFile(path: string, destDir: string, newName: string | null) {
    const item = pending.find((p) => p.path === path) ?? inbox.find((i) => i.path === path);
    if (!item) return fail("파일을 찾을 수 없어요. 이미 옮겨졌거나 지워졌을 수 있어요.");
    const dest = findFolder(destDir);
    if (dest) dest.files += 1;
    const id = ++nextId;
    moves.push({ id, original: { ...item }, destDir, undone: false });
    pending = pending.filter((p) => p.path !== path);
    inbox = inbox.filter((i) => i.path !== path);
    recent = [destDir, ...recent.filter((r) => r !== destDir)];
    mockEvents.emit("inbox-changed");
    return delay<PutResult>({ moveId: id, movedPath: destDir + SEP + (newName ?? item.name), destDir });
  },

  undoMove(moveId: number, requeue: boolean) {
    const move = moves.find((m) => m.id === moveId);
    if (!move) return fail("되돌릴 기록을 찾을 수 없어요.");
    if (move.undone) return fail("이미 되돌렸어요.");
    move.undone = true;
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
    const out = [...new Set([...frequent, ...seed])].filter((p) => findFolder(p));
    if (root && tree) {
      for (const name of Object.keys(tree.children).sort((a, b) => a.localeCompare(b, "ko"))) {
        if (out.length >= 6) break;
        const p = root + SEP + name;
        if (!out.includes(p)) out.push(p);
      }
    }
    return delay(out.slice(0, 6));
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

  listInbox: () => delay([...inbox]),

  pickFolder() {
    pickCount += 1;
    return delay(`${HOME}\\Documents\\고른 폴더 ${pickCount}`);
  },
};
