import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, Lightbulb, Wand2, Zap } from "lucide-react";
import ChoiceCard from "../components/ChoiceCard";
import FolderPane, { type Toast } from "../components/FolderPane";
import ModeChoice from "../components/ModeChoice";
import WatchFolderEditor from "../components/WatchFolderEditor";
import { errorMessage, getAppInfo } from "../lib/backend";
import type { OrganizeMode, PlaceMode } from "../lib/types";
import { useAppStore } from "../store/useAppStore";
import { useUpdateStore } from "../store/useUpdateStore";
import "./SettingsPage.css";

type SavedKey = "mode" | "watch" | "place" | "term";

const MONTHS = Array.from({ length: 12 }, (_, i) => i + 1);

// 설정(SPEC 3-3). 바꾸면 바로 저장한다. 나머지 항목은 6단계에서 늘어난다.
export default function SettingsPage() {
  const settings = useAppStore((s) => s.settings);
  const saveMode = useAppStore((s) => s.setOrganizeMode);
  const saveWatch = useAppStore((s) => s.setWatchFolders);
  const savePlace = useAppStore((s) => s.setPlaceMode);
  const saveTerm = useAppStore((s) => s.setTermMonths);
  const known = useAppStore((s) => s.setupInfo);
  const [watchDraft, setWatchDraft] = useState<string[]>(settings?.watchFolders ?? []);
  const [saved, setSaved] = useState<SavedKey | null>(null);
  const [errors, setErrors] = useState<Partial<Record<SavedKey, string>>>({});
  const [version, setVersion] = useState<string | null>(null);
  const [folderToast, setFolderToast] = useState<Toast | null>(null);
  const savedTimer = useRef<number>();

  useEffect(() => {
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
    <div className="page settings">
      <h1 className="page__title">설정</h1>

      <Section title="새 파일이 생기면" saved={saved === "mode"} error={errors.mode}>
        <ModeChoice value={settings.organizeMode} onChange={changeMode} />
      </Section>

      <Section
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

      <Section
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

      <Section
        title="지켜볼 폴더"
        description="골라 둔 폴더에 바로 저장되는 파일만 봐요."
        saved={saved === "watch"}
        error={errors.watch}
      >
        <WatchFolderEditor known={known} value={watchDraft} onChange={changeWatch} />
      </Section>

      <Section
        title="폴더 구성"
        description="정리 폴더 안의 폴더를 만들고, 이름을 바꾸고, 옮기고, 지울 수 있어요. 폴더에 마우스를 올리면 나오는 ⋯ 를 누르세요. 지운 폴더는 휴지통으로 가요."
      >
        <div className="settings__path">{settings.rootFolder}</div>
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

      <Section title="업데이트" description="새 버전이 나오면 프로그램이 알아서 알려 드려요(켤 때와 하루에 한 번 확인).">
        <UpdateRow version={version} />
      </Section>
    </div>
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
  title,
  description,
  saved,
  error,
  children,
}: {
  title: string;
  description?: string;
  saved?: boolean;
  error?: string;
  children: ReactNode;
}) {
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
