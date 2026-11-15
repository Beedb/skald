import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "../docs/temas.css";
import "@fontsource/cinzel/600.css";
import "@fontsource/cinzel/700.css";
import "@fontsource/lora/500.css";
import "@fontsource/lora/600.css";
import { iniciarTema } from "./tema";
import { acompanharMargens } from "./android";

iniciarTema();

acompanharMargens();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
