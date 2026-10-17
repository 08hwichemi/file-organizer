// 끌어다 놓기 공통(정리함·자동 분류 창): 끌리는 파일 표시와 놓을 곳을 마우스에 맞춘다.
// 사용자 보고: 긴 줄의 오른쪽을 잡고 끌면 표시가 마우스에서 멀리 떨어지고, 표시가 겹친 엉뚱한 폴더에 들어갔다.
// → 표시는 followCursor로 마우스 옆에, 놓을 곳은 DndContext의 collisionDetection={pointerWithin}(마우스 아래 칸)으로.
import type { Modifier } from "@dnd-kit/core";

/** 끌리는 표시를 마우스 오른쪽 아래에 붙인다(마우스 아래 폴더 이름을 가리지 않게). */
export const followCursor: Modifier = ({ activatorEvent, activeNodeRect, transform }) => {
  if (!activeNodeRect || !(activatorEvent instanceof PointerEvent)) return transform;
  return {
    ...transform,
    x: transform.x + activatorEvent.clientX - activeNodeRect.left + 14,
    y: transform.y + activatorEvent.clientY - activeNodeRect.top + 14,
  };
};

