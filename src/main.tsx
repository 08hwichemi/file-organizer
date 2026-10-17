import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { keepMiddleDotsTogether } from "./lib/keepDots";
import "./styles/global.css";

const root = document.getElementById("root") as HTMLElement;
// "학년도·학기(3월 / ·9월)"처럼 가운뎃점에서 줄이 바뀌지 않게(화면 전체, 대화 창 포함)
keepMiddleDotsTogether(document.body);

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
