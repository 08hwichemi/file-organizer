import { BellRing, Inbox } from "lucide-react";
import type { OrganizeMode } from "../lib/types";
import ChoiceCard from "./ChoiceCard";

interface Props {
  value: OrganizeMode;
  onChange: (mode: OrganizeMode) => void;
}

/** 새 파일이 생겼을 때 방식 고르기. 처음 실행과 설정에서 같이 쓴다. */
export default function ModeChoice({ value, onChange }: Props) {
  return (
    <div className="stack-12" role="radiogroup" aria-label="새 파일이 생기면">
      <ChoiceCard
        name="organize-mode"
        checked={value === "ask"}
        onSelect={() => onChange("ask")}
        icon={<BellRing size={20} />}
        title="바로 물어보기"
        description="새 파일이 저장될 때마다 화면 오른쪽 아래에 작은 창을 띄워 넣을 곳을 물어봐요. 나중에 하고 싶으면 ‘나중에 정리’를 누르면 돼요."
      />
      <ChoiceCard
        name="organize-mode"
        checked={value === "manual"}
        onSelect={() => onChange("manual")}
        icon={<Inbox size={20} />}
        title="모아 두고 나중에 정리"
        description="창을 띄우지 않고 새 파일을 정리함에 모아 둬요. 원할 때 프로그램을 열어 한꺼번에 정리해요."
      />
    </div>
  );
}
