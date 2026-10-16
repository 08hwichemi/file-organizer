import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import {
  Briefcase,
  Check,
  FolderTree,
  Info,
  Lightbulb,
  RotateCcw,
  School,
  Search,
  SlidersHorizontal,
  Wand2,
  Zap,
  type LucideIcon,
} from "lucide-react";
import ChoiceCard from "../components/ChoiceCard";
import Dialog from "../components/Dialog";
import FavoriteSettings from "../components/FavoriteSettings";
import FolderPane, { type Toast } from "../components/FolderPane";
import ModeChoice from "../components/ModeChoice";
import WatchFolderEditor from "../components/WatchFolderEditor";
import { errorMessage, getAppInfo, getSearchInfo, pickFolder } from "../lib/backend";
import type { OrganizeMode, PlaceMode, SearchInfo } from "../lib/types";
import { useAppStore, type SettingsTab } from "../store/useAppStore";
import { useUpdateStore } from "../store/useUpdateStore";
import terms from "../../src-tauri/resources/TERMS.txt?raw";
import thirdParty from "../../src-tauri/resources/THIRD_PARTY.txt?raw";
import "./SettingsPage.css";

type SavedKey = "kind" | "mode" | "watch" | "place" | "term" | "wait" | "root" | "fav" | "autostart" | "search" | "scope";

/** 검색 단축키로 고를 수 있는 것("" = 쓰지 않음) */
const SHORTCUTS = ["Alt+Space", "Ctrl+Alt+F", "Ctrl+Shift+Space", ""];

/** 지금 무엇으로 찾는지(설정 화면 아래 한 줄) */
function searchStatus(info: SearchInfo, wholePc: boolean): string {
  const count = (n: number) => n.toLocaleString("ko-KR");
  if (info.everything === "ready") return "Everything이 켜져 있어 Everything으로 찾아요(가장 빨라요).";
  if (wholePc && info.indexing) return `PC 전체를 읽는 중이에요(파일 ${count(info.indexed)}개). 다 읽을 때까지는 정리 폴더와 지켜볼 폴더에서 찾아요.`;
  if (wholePc && info.wholePc) return `PC 전체의 파일 ${count(info.indexed)}개에서 찾아요.`;
  if (wholePc) return "파일 검색을 처음 열 때 PC 전체를 읽기 시작해요.";
  return `정리 폴더와 지켜볼 폴더의 파일 ${count(info.indexed)}개에서 찾아요.`;
}

/** "응답이 없으면 n초 뒤" 고를 수 있는 값(0 = 끔) */
const WAIT_CHOICES = [0, 5, 10, 20, 30];

const MONTHS = Array.from({ length: 12 }, (_, i) => i + 1);

/** 설정을 네 갈래로 나눠 탭으로 보여 준다(한 화면에 다 있으면 너무 길어서 — 사용자 요청) */
const TABS: { key: SettingsTab; label: string; Icon: LucideIcon }[] = [
  { key: "organize", label: "정리 방식", Icon: SlidersHorizontal },
  { key: "folders", label: "폴더", Icon: FolderTree },
  { key: "search", label: "파일 검색", Icon: Search },
  { key: "app", label: "프로그램", Icon: Info },
];

/** 지금 보이는 탭(Section이 자기 탭일 때만 그려지게) */
const TabContext = createContext<SettingsTab>("organize");

// 설정(SPEC 3-3). 바꾸면 바로 저장한다.
export default function SettingsPage() {
  const settings = useAppStore((s) => s.settings);
  const tab = useAppStore((s) => s.settingsTab);
  const setTab = useAppStore((s) => s.setSettingsTab);
  const redoSetup = useAppStore((s) => s.redoSetup);
  const saveMode = useAppStore((s) => s.setOrganizeMode);
  const saveWatch = useAppStore((s) => s.setWatchFolders);
  const savePlace = useAppStore((s) => s.setPlaceMode);
  const saveTerm = useAppStore((s) => s.setTermMonths);
  const saveAutostart = useAppStore((s) => s.setAutostart);
  const saveWait = useAppStore((s) => s.setWaitSeconds);
  const savePins = useAppStore((s) => s.setPinnedFavorites);
  const saveRoot = useAppStore((s) => s.setRootFolder);
  const [newRoot, setNewRoot] = useState<string | null>(null);
  const [showLicenses, setShowLicenses] = useState(false);
  const [showTerms, setShowTerms] = useState(false);
  const saveShortcut = useAppStore((s) => s.setSearchShortcut);
  const saveWholePc = useAppStore((s) => s.setSearchWholePc);
  const saveKind = useAppStore((s) => s.setUserKind);
  const openSearch = useAppStore((s) => s.openSearch);
  const [searchInfo, setSearchInfo] = useState<SearchInfo | null>(null);
  const known = useAppStore((s) => s.setupInfo);
  const [watchDraft, setWatchDraft] = useState<string[]>(settings?.watchFolders ?? []);
  const [saved, setSaved] = useState<SavedKey | null>(null);
  const [errors, setErrors] = useState<Partial<Record<SavedKey, string>>>({});
  const [version, setVersion] = useState<string | null>(null);
  const [folderToast, setFolderToast] = useState<Toast | null>(null);
  const savedTimer = useRef<number>();

  useEffect(() => {
    getSearchInfo()
      .then(setSearchInfo)
      .catch(() => setSearchInfo(null));
    getAppInfo()
      .then((info) => setVersion(info.version))
      .catch(() => setVersion(null));
    return () => window.clearTimeout(savedTimer.current);
  }, []);

  if (!settings) return null;

  const flashSaved = (which: SavedKey) => {
    setSaved(which);
    window.clearTimeout(savedTimer.current);
    savedTimer.current = window.setTimeout(() => setSaved(null), 2000);
  };

  const changeMode = async (mode: OrganizeMode) => {
    if (mode === settings.organizeMode) return;
    try {
      await saveMode(mode);
      setErrors((e) => ({ ...e, mode: undefined }));
      flashSaved("mode");
    } catch (e) {
      setErrors((prev) => ({ ...prev, mode: errorMessage(e) }));
    }
  };

  /** 저장하고 "저장했어요" 또는 오류를 그 칸에 보여 준다 */
  const save = async (key: SavedKey, work: () => Promise<void>) => {
    try {
      await work();
      setErrors((e) => ({ ...e, [key]: undefined }));
      flashSaved(key);
    } catch (e) {
      setErrors((prev) => ({ ...prev, [key]: errorMessage(e) }));
    }
  };

  const chooseRoot = async () => {
    try {
      const picked = await pickFolder("새 정리 폴더 고르기");
      if (picked) setNewRoot(picked);
    } catch (e) {
      setErrors((prev) => ({ ...prev, root: errorMessage(e) }));
    }
  };

  const changePlace = async (mode: PlaceMode) => {
    if (mode === settings.placeMode) return;
    try {
      await savePlace(mode);
      setErrors((e) => ({ ...e, place: undefined }));
      flashSaved("place");
    } catch (e) {
      setErrors((prev) => ({ ...prev, place: errorMessage(e) }));
    }
  };

  const changeTerm = async (first: number, second: number) => {
    try {
      await saveTerm(first, second);
      setErrors((e) => ({ ...e, term: undefined }));
      flashSaved("term");
    } catch (e) {
      setErrors((prev) => ({ ...prev, term: errorMessage(e) }));
    }
  };

  const changeWatch = async (folders: string[]) => {
    setWatchDraft(folders);
    if (folders.length === 0) {
      setErrors((e) => ({ ...e, watch: "지켜볼 폴더를 하나 이상 골라 주세요." }));
      return;
    }
    try {
      await saveWatch(folders);
      setErrors((e) => ({ ...e, watch: undefined }));
      flashSaved("watch");
    } catch (e) {
      setErrors((prev) => ({ ...prev, watch: errorMessage(e) }));
    }
  };

  return (
    <TabContext.Provider value={tab}>
    <div className="page settings">
      <h1 className="page__title">설정</h1>

      <div className="settings__tabs" role="tablist" aria-label="설정 갈래">
        {TABS.map(({ key, label, Icon }) => (
          <button
            key={key}
            type="button"
            role="tab"
            aria-selected={tab === key}
            className={`settings__tab${tab === key ? " settings__tab--on" : ""}`}
            onClick={() => setTab(key)}
          >
            <Icon size={16} aria-hidden="true" />
            {label}
          </button>
        ))}
      </div>

      <Section tab="organize"
        title="쓰는 곳"
        description="자동 분류 낱말(고친 적 없을 때)과 연도를 정하는 방식이 달라져요. 이미 만든 폴더는 그대로 둬요."
        saved={saved === "kind"}
        error={errors.kind}
      >
        <div className="settings__kinds" role="radiogroup" aria-label="쓰는 곳">
          <ChoiceCard
            name="user-kind"
            checked={settings.userKind !== "general"}
            onSelect={() => void save("kind", () => saveKind("school"))}
            icon={<School size={20} />}
            title="학교"
            description="공문·수행평가 같은 낱말, 학년도·학기(3월·9월)"
          />
          <ChoiceCard
            name="user-kind"
            checked={settings.userKind === "general"}
            onSelect={() => void save("kind", () => saveKind("general"))}
            icon={<Briefcase size={20} />}
            title="일반(회사·개인)"
            description="보고서·영수증·세금 같은 낱말, 연도는 달력의 해"
          />
        </div>
      </Section>

      <Section tab="organize" title="새 파일이 생기면" saved={saved === "mode"} error={errors.mode}>
        <ModeChoice value={settings.organizeMode} onChange={changeMode} />
      </Section>

      <Section tab="organize"
        title="정리 방식"
        description="새 파일을 얼마나 알아서 정리할지 골라요. 무엇을 기준으로 나누는지는 왼쪽 ‘정리 규칙’에서 보고 고칠 수 있어요."
        saved={saved === "place"}
        error={errors.place}
      >
        <div className="stack-12" role="radiogroup" aria-label="정리 방식">
          <ChoiceCard
            name="place-mode"
            checked={settings.placeMode === "suggest"}
            onSelect={() => void changePlace("suggest")}
            icon={<Lightbulb size={20} />}
            title="추천만"
            description="팝업에 추천 위치를 골라 둔 채로 물어봐요. ‘넣기’만 누르면 돼요."
          />
          <ChoiceCard
            name="place-mode"
            checked={settings.placeMode === "auto"}
            onSelect={() => void changePlace("auto")}
            icon={<Zap size={20} />}
            title="바로 정리"
            description="직접 만든 정리 규칙에 맞는 파일은 묻지 않고 바로 넣은 뒤, 화면 오른쪽 아래에 알려 드려요. 잘못 넣었으면 그 자리에서 되돌릴 수 있어요. 나머지는 위의 방식대로요."
          />
          <ChoiceCard
            name="place-mode"
            checked={settings.placeMode === "classify"}
            onSelect={() => void changePlace("classify")}
            icon={<Wand2 size={20} />}
            title="자동 분류"
            description="규칙이 없어도 파일 이름(폴더 이름·자동 분류 단어)과 형식(한글·PDF·사진 등)을 보고 알아서 나눠요. 없는 폴더는 만들고, 넣은 뒤 오른쪽 아래에 알려 드려요(되돌리기 가능). 구별이 안 되는 파일만 정리함에 모여요."
          />
        </div>
        {settings.placeMode === "suggest" && (
          <label className="settings__wait">
            <span>팝업에 응답이 없으면</span>
            <select
              value={settings.waitSeconds}
              onChange={(e) => void save("wait", () => saveWait(Number(e.target.value)))}
            >
              {WAIT_CHOICES.map((n) => (
                <option key={n} value={n}>
                  {n === 0 ? "그대로 기다리기" : `${n}초 뒤 추천 위치에 넣기`}
                </option>
              ))}
            </select>
          </label>
        )}
        {errors.wait && <p className="error-text">{errors.wait}</p>}
        {settings.placeMode === "classify" && (
          <p className="settings__note">
            자동 분류 중에는 ‘새 파일이 생기면’ 설정과 상관없이 묻지 않고 나눠요. 이미 있던 파일은 정리함의 ‘자동 분류’로
            나눌 수 있어요.
          </p>
        )}
        {settings.organizeMode === "manual" && settings.placeMode === "auto" && (
          <p className="settings__note">
            지금은 ‘모아 두고 나중에 정리’라서 새 파일은 모두 정리함으로 가요. 정리함의 ‘자동 분류’로 한꺼번에 나눌 수
            있어요.
          </p>
        )}
      </Section>

      {settings.userKind !== "general" && (
      <Section tab="organize"
        title="학년도·학기"
        description={`규칙이 연도·학기 폴더를 나눌 때, 파일 이름에 연도·학기가 없으면 오늘 날짜로 정해요. ${termRule(
          settings.firstTermMonth,
          settings.secondTermMonth,
        )}`}
        saved={saved === "term"}
        error={errors.term}
      >
        <div className="settings__terms">
          <label className="settings__term">
            <span>1학기 시작</span>
            <select
              value={settings.firstTermMonth}
              onChange={(e) => void changeTerm(Number(e.target.value), settings.secondTermMonth)}
            >
              {MONTHS.map((m) => (
                <option key={m} value={m} disabled={m >= settings.secondTermMonth}>
                  {m}월
                </option>
              ))}
            </select>
          </label>
          <label className="settings__term">
            <span>2학기 시작</span>
            <select
              value={settings.secondTermMonth}
              onChange={(e) => void changeTerm(settings.firstTermMonth, Number(e.target.value))}
            >
              {MONTHS.map((m) => (
                <option key={m} value={m} disabled={m <= settings.firstTermMonth}>
                  {m}월
                </option>
              ))}
            </select>
          </label>
        </div>
      </Section>

      )}

      <Section tab="folders"
        title="지켜볼 폴더"
        description="골라 둔 폴더에 바로 저장되는 파일만 봐요."
        saved={saved === "watch"}
        error={errors.watch}
      >
        <WatchFolderEditor known={known} value={watchDraft} onChange={changeWatch} />
      </Section>

      <Section tab="folders"
        title="정리 폴더"
        description="정리 폴더 안의 폴더를 만들고, 이름을 바꾸고, 옮기고, 지울 수 있어요. 폴더에 마우스를 올리면 나오는 ⋯ 를 누르세요. 지운 폴더는 휴지통으로 가요."
        saved={saved === "root"}
        error={errors.root}
      >
        <div className="settings__root button-row">
          <div className="settings__path">{settings.rootFolder}</div>
          <button type="button" className="btn" onClick={() => void chooseRoot()}>
            다른 폴더로 바꾸기
          </button>
        </div>
        {settings.rootFolder && (
          <FolderPane
            key={settings.rootFolder}
            root={settings.rootFolder}
            separator={known?.separator ?? "\\"}
            toast={folderToast}
            onToast={setFolderToast}
            label="폴더 구성"
          />
        )}
      </Section>

      {settings.rootFolder && (
        <Section tab="folders"
          title="자주 쓰는 폴더"
          description="정리함에서 파일을 고르고 숫자 키 1~6을 누르면 이 폴더에 들어가요. 고르지 않은 칸은 많이 넣은 폴더부터 자동으로 채워요."
          saved={saved === "fav"}
          error={errors.fav}
        >
          <FavoriteSettings
            root={settings.rootFolder}
            separator={known?.separator ?? "\\"}
            pins={settings.pinnedFavorites}
            onChange={(pins) => save("fav", () => savePins(pins))}
          />
        </Section>
      )}

      <Section tab="search"
        title="파일 검색"
        description="왼쪽 메뉴의 ‘파일 검색’에서 찾아요. 단축키를 누르면 다른 프로그램을 쓰다가도 바로 이 창의 검색 화면이 열려요."
        saved={saved === "search" || saved === "scope"}
        error={errors.search ?? errors.scope ?? searchInfo?.shortcutError ?? undefined}
      >
        <label className="settings__toggle">
          <input
            type="checkbox"
            checked={settings.searchWholePc}
            onChange={(e) =>
              void save("scope", async () => {
                await saveWholePc(e.target.checked);
                setSearchInfo(await getSearchInfo().catch(() => null));
              })
            }
          />
          <span>
            <strong>PC 전체에서 찾기</strong>
            <span className="settings__toggle-desc">
              Everything을 깔지 않아도 이 프로그램이 PC의 파일 목록을 직접 읽어 찾아요(Windows·프로그램 폴더 같은
              시스템 폴더는 빼요). 처음 읽을 때는 몇 분 걸릴 수 있지만, 컴퓨터가 느려지지 않게 뒤에서 천천히 읽어요.
              끄면 정리 폴더와 지켜볼 폴더에서만 찾아요.
            </span>
          </span>
        </label>
        <div className="settings__search button-row">
          <label className="settings__wait">
            <span>단축키</span>
            <select
              value={settings.searchShortcut}
              onChange={(e) => void save("search", () => saveShortcut(e.target.value))}
            >
              {SHORTCUTS.map((k) => (
                <option key={k} value={k}>
                  {k === "" ? "쓰지 않기" : k}
                </option>
              ))}
            </select>
          </label>
          <span className="popup__spacer" />
          <button type="button" className="btn" onClick={openSearch}>
            파일 검색 열기
          </button>
        </div>
        {searchInfo && <p className="settings__note settings__note--plain">{searchStatus(searchInfo, settings.searchWholePc)}</p>}
      </Section>

      <Section tab="app" title="자동 실행" saved={saved === "autostart"} error={errors.autostart}>
        <label className="settings__toggle">
          <input
            type="checkbox"
            checked={settings.autostart}
            onChange={(e) => void save("autostart", () => saveAutostart(e.target.checked))}
          />
          <span>
            <strong>컴퓨터를 켜면 자동으로 실행</strong>
            <span className="settings__toggle-desc">
              창 없이 화면 오른쪽 아래 트레이에서 조용히 시작해 새 파일을 지켜봐요. 창의 X를 눌러도 꺼지지 않고 트레이에서
              계속 지켜봐요. 완전히 끄려면 트레이 아이콘 → ‘끄기’를 누르세요.
            </span>
          </span>
        </label>
      </Section>

      <Section tab="app" title="업데이트" description="새 버전이 나오면 프로그램이 알아서 알려 드려요(켤 때와 하루에 한 번 확인).">
        <UpdateRow version={version} />
      </Section>

      <Section tab="app"
        title="처음 설정 다시 하기"
        description="처음 켰을 때 고른 것(쓰는 곳, 정리 폴더, 예시 폴더, 지켜볼 폴더, 새 파일 알림 방식)을 한 번에 다시 골라요. 지금 설정이 골라진 채로 열리고, 정리 규칙·자동 분류 단어·기록과 폴더·파일은 그대로 둬요."
      >
        <div className="button-row">
          <button type="button" className="btn" onClick={() => redoSetup(true)}>
            <RotateCcw size={16} aria-hidden="true" />
            처음 설정 다시 하기
          </button>
        </div>
      </Section>

      <Section tab="app" title="정보">
        <div className="settings__about">
          <div className="settings__about-text">
            <strong>파일 정리 도우미</strong>
            <span className="settings__about-line">
              {version ? `버전 ${version}` : "버전 확인 중"} · 만든 사람 이용휘(화학 교사)
            </span>
            <span className="settings__toggle-desc">
              파일 이름과 경로는 이 PC 안에서만 쓰고 밖으로 보내지 않아요(인터넷은 업데이트 확인에만 써요).
            </span>
            <span className="settings__toggle-desc">교육·개인·업무 용도로 무료로 쓸 수 있어요. 판매는 할 수 없어요.</span>
            <span className="settings__copyright">© 2026 이용휘</span>
          </div>
          <div className="settings__about-actions button-row">
            <button type="button" className="btn" onClick={() => setShowTerms(true)}>
              이용 조건
            </button>
            <button type="button" className="btn" onClick={() => setShowLicenses(true)}>
              오픈소스 라이선스
            </button>
          </div>
        </div>
      </Section>

      {showTerms && (
        <Dialog
          title="이용 조건"
          wide
          className="dialog--fit"
          onClose={() => setShowTerms(false)}
          actions={
            <button type="button" className="btn btn--primary" onClick={() => setShowTerms(false)}>
              닫기
            </button>
          }
        >
          <pre className="settings__licenses">{terms}</pre>
        </Dialog>
      )}

      {showLicenses && (
        <Dialog
          title="오픈소스 라이선스"
          wide
          onClose={() => setShowLicenses(false)}
          actions={
            <button type="button" className="btn btn--primary" onClick={() => setShowLicenses(false)}>
              닫기
            </button>
          }
        >
          <pre className="settings__licenses">{thirdParty}</pre>
        </Dialog>
      )}

      {newRoot && (
        <Dialog
          title="정리 폴더를 바꿀까요?"
          onClose={() => setNewRoot(null)}
          actions={
            <>
              <button type="button" className="btn btn--ghost" onClick={() => setNewRoot(null)}>
                취소
              </button>
              <button
                type="button"
                className="btn btn--primary"
                onClick={() => {
                  const path = newRoot;
                  setNewRoot(null);
                  void save("root", () => saveRoot(path));
                }}
              >
                바꾸기
              </button>
            </>
          }
        >
          <p>
            앞으로 <strong>{newRoot}</strong> 폴더에 정리해요.
          </p>
          <p className="dialog__sub">
            지금 정리 폴더와 그 안의 파일은 그대로 있어요(옮기지 않아요). 직접 고른 자주 쓰는 폴더는 비워져요. 예전 폴더로
            가는 정리 규칙은 그대로 예전 폴더에 넣어요.
          </p>
        </Dialog>
      )}
    </div>
    </TabContext.Provider>
  );
}

/** "3~8월은 1학기, 9~12월은 2학기, 1~2월은 전 학년도 2학기예요." */
function termRule(first: number, second: number): string {
  const range = (a: number, b: number) => (a === b ? `${a}월` : `${a}~${b}월`);
  const parts = [`${range(first, second - 1)}은 1학기`, `${range(second, 12)}은 2학기`];
  if (first > 1) parts.push(`${range(1, first - 1)}은 전 학년도 2학기`);
  return `${parts.join(", ")}예요.`;
}

function Section({
  tab,
  title,
  description,
  saved,
  error,
  children,
}: {
  /** 이 칸이 있는 탭 */
  tab: SettingsTab;
  title: string;
  description?: string;
  saved?: boolean;
  error?: string;
  children: ReactNode;
}) {
  if (useContext(TabContext) !== tab) return null;
  return (
    <section className="settings__section">
      <div className="settings__head">
        <h2 className="settings__title">{title}</h2>
        <span className="settings__saved" aria-live="polite">
          {saved && (
            <>
              <Check size={14} aria-hidden="true" />
              저장했어요
            </>
          )}
        </span>
      </div>
      {description && <p className="settings__desc">{description}</p>}
      {children}
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}

function UpdateRow({ version }: { version: string | null }) {
  const { status, version: newVersion, error, check } = useUpdateStore();
  const message =
    status === "checking"
      ? "확인하는 중…"
      : status === "latest"
        ? "지금 최신 버전이에요."
        : status === "available" || status === "downloading"
          ? `새 버전 ${newVersion}이 있어요. 위의 ‘지금 업데이트’를 눌러 주세요.`
          : status === "error"
            ? error
            : null;
  return (
    <div className="settings__update button-row">
      <div className="settings__update-text">
        <div>지금 버전 {version ?? "…"}</div>
        {message && <div className="settings__update-msg">{message}</div>}
      </div>
      <button type="button" className="btn" onClick={() => void check()} disabled={status === "checking" || status === "downloading"}>
        업데이트 확인
      </button>
    </div>
  );
}
