import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import StartupGate from "./StartupGate";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <StartupGate><App /></StartupGate>
  </StrictMode>,
);
