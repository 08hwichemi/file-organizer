import "./WordMatchChoice.css";

interface Props {
  anyWord: boolean;
  onChange: (anyWord: boolean) => void;
}

/** 단어가 여럿일 때: 모두 들어 있어야 맞는지, 하나라도 있으면 맞는지(SPEC 4장) */
export default function WordMatchChoice({ anyWord, onChange }: Props) {
  return (
    <div className="word-match" role="radiogroup" aria-label="단어가 여럿일 때">
      <button
        type="button"
        role="radio"
        aria-checked={!anyWord}
        className={`word-match__opt${anyWord ? "" : " word-match__opt--on"}`}
        onClick={() => onChange(false)}
      >
        모두 들어 있으면
      </button>
      <button
        type="button"
        role="radio"
        aria-checked={anyWord}
        className={`word-match__opt${anyWord ? " word-match__opt--on" : ""}`}
        onClick={() => onChange(true)}
      >
        하나라도 있으면
      </button>
    </div>
  );
}
