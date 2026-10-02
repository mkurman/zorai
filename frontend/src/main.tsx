import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { reclaimIdleAgentWorkspaces } from "./lib/agentWorkspaceReclaim";
import { loadSession, saveSession } from "./lib/sessionPersistence";
import { hydrateCommandLogStore } from "./lib/commandLogStore";
import { hydrateAgentMissionStore } from "./lib/agentMissionStore";
import { hydrateKeybindStore } from "./lib/keybindStore";
import { hydrateSettingsStore } from "./lib/settingsStore";
import { hydrateAgentStore } from "./lib/agentStore";
import { hydrateTranscriptStore } from "./lib/transcriptStore";
import { hydrateFileManagerStore } from "./lib/fileManagerStore";
import { hydrateSnippetStore } from "./lib/snippetStore";
import { hydrateStatusStore } from "./lib/statusStore";
import { hydrateTierStore } from "./lib/tierStore";
import { useWorkspaceStore } from "./lib/workspaceStore";
import "./styles/global.css";

const setBootStatus = (text: string): void => {
  const status = document.getElementById("zorai-boot-status");
  if (status) status.textContent = text;
};

const dismissBootSplash = (): void => {
  const splash = document.getElementById("zorai-boot");
  if (!splash) return;
  splash.classList.add("zorai-boot--done");
  window.setTimeout(() => splash.remove(), 500);
};

const renderRoot = (): void => {
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>
  );
  // Let the first paint land before fading the splash out.
  requestAnimationFrame(() => requestAnimationFrame(dismissBootSplash));
};

async function bootstrap() {
  setBootStatus("Loading settings…");
  await hydrateSettingsStore();

  setBootStatus("Hydrating agent stores…");
  await Promise.all([
    hydrateAgentStore(),
    hydrateCommandLogStore(),
    hydrateAgentMissionStore(),
    hydrateKeybindStore(),
    hydrateTranscriptStore(),
    hydrateFileManagerStore(),
    hydrateSnippetStore(),
    hydrateTierStore(),
  ]);

  // Start status polling after stores are hydrated (non-blocking)
  hydrateStatusStore();

  setBootStatus("Restoring session…");
  const persistedSession = await loadSession();
  if (persistedSession) {
    useWorkspaceStore.getState().hydrateSession(persistedSession);
  }
  await reclaimIdleAgentWorkspaces();
  saveSession();

  setBootStatus("Launching interface…");
  renderRoot();
}

void bootstrap();
