// 화면 글이 어색한 곳에서 줄이 바뀌지 않게 한다(화면 전체에 한 번 걸어 두면 새로 그려진 글에도 적용).
// - 가운뎃점(·)으로 이은 말("학년도·학기(3월·9월)", "업무·수업·평가"): 브라우저(WebView2)는 ·의 앞뒤를
//   줄바꿈 자리로 보아 "(3월 / ·9월)"처럼 끊으므로, · 양쪽에 보이지 않는 낱말 이음표(U+2060)를 붙인다.
// - 빈칸이 든 짧은 작은따옴표 말(‘나중에 정리’, ‘이름 (2)’): 가운데서 끊기지 않게 그 안의 빈칸을 줄바꿈 없는 빈칸으로.
// 입력 칸 안의 글은 건드리지 않는다.

const JOIN = "\u2060";
const NBSP = "\u00a0";
const LOOSE_DOT = /(?<!\u2060)·|·(?!\u2060)/;
const ANY_DOT = /\u2060?·\u2060?/g;
/** 빈칸이 든 짧은(앞뒤 12자까지) ‘…’ */
const SHORT_QUOTE = /‘[^’\n]{0,12} [^’\n]{0,12}’/;
const SHORT_QUOTES = /‘[^’\n]{1,25}’/g;

function fixText(node: Text) {
  const loose = LOOSE_DOT.test(node.data);
  const quote = SHORT_QUOTE.test(node.data);
  if (!loose && !quote) return;
  const parent = node.parentElement;
  if (!parent || parent.closest("input, textarea, [contenteditable='true']")) return;
  let text = node.data;
  if (loose) text = text.replace(ANY_DOT, `${JOIN}·${JOIN}`);
  if (quote) text = text.replace(SHORT_QUOTES, (q) => q.replace(/ /g, NBSP));
  if (text !== node.data) node.data = text;
}

function fixTree(root: Node) {
  if (root.nodeType === Node.TEXT_NODE) {
    fixText(root as Text);
    return;
  }
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let node: Node | null;
  while ((node = walker.nextNode())) fixText(node as Text);
}

/** 한 번 부르면 root 아래의 지금 글과 앞으로 그려질 글 모두에 적용된다. */
export function keepMiddleDotsTogether(root: Node) {
  fixTree(root);
  new MutationObserver((records) => {
    for (const r of records) {
      if (r.type === "characterData") fixText(r.target as Text);
      else r.addedNodes.forEach(fixTree);
    }
  }).observe(root, { subtree: true, childList: true, characterData: true });
}
